//! Quellcode finden, bauen und den Stand pruefen -- fuer Horizon und BHH.
//!
//! Gebaut wird mit dem `cargo` des Rechners in einen eigenen Zielordner; die
//! fertigen Programme landen in `<variante>/bin/`. Ob sie zum Quellcode passen,
//! merkt sich `bin/stand.json`. BHH baut zusaetzlich seine Java-Seite
//! (`bhh-api.jar`) mit dem JDK 25, das BuildMC verwaltet.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::Variant;
use crate::error::{Error, Result};
use crate::paths;

/// Der Quellcode einer Variante: Umgebungsvariable, sonst neben dem BuildMC-Quellcode
/// (Entwicklung), sonst im Datenordner.
pub fn source_dir(v: Variant) -> Option<PathBuf> {
    let (var, neben, merkmal) = match v {
        Variant::Horizon => ("BUILDMC_HORIZON_SRC", "../rust-edition/horizon", "crates/horizon-client"),
        Variant::Bhh => ("BUILDMC_BHH_SRC", "../bhh", "crates/bhh-client"),
    };
    let mut kandidaten = Vec::new();
    if let Some(p) = std::env::var_os(var).filter(|p| !p.is_empty()) {
        kandidaten.push(PathBuf::from(p));
    }
    kandidaten.push(Path::new(env!("CARGO_MANIFEST_DIR")).join(neben));
    if let Ok(r) = v.root() {
        kandidaten.push(r.join("source"));
    }
    kandidaten.into_iter()
        .find(|p| p.join("Cargo.toml").exists() && p.join("research-lock.json").exists() && p.join(merkmal).exists())
        .map(|p| p.canonicalize().unwrap_or(p))
}

/// Stand des Quellcodes: bei Horizon der Commit aus `VENDORED.md`, bei BHH der Git-Commit des Forks.
pub fn source_revision(v: Variant, src: &Path) -> String {
    let rev = match v {
        Variant::Horizon => src.parent().map(|p| p.join("VENDORED.md"))
            .and_then(|n| std::fs::read_to_string(n).ok())
            .and_then(|t| t.lines().find(|l| l.contains("Übernommener Stand")).map(|l| {
                l.split('`').nth(1).unwrap_or("").split_whitespace().next().unwrap_or("").to_string()
            })),
        Variant::Bhh => Command::new("git").current_dir(src).args(["rev-parse", "HEAD"]).output().ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()),
    };
    rev.filter(|s| !s.is_empty()).unwrap_or_else(|| "unbekannt".into())
}

static RESSOURCEN: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Ordner der mitgelieferten Rust-Editionen in den Anwendungs-Ressourcen (beim Start gesetzt).
pub fn set_resource_dir(dir: PathBuf) {
    let _ = RESSOURCEN.set(dir);
}

/// Die mitgelieferte, fertig gebaute Variante (`<ressourcen>/<horizon|bhh>/`), falls vorhanden.
fn mitgeliefert(v: Variant) -> Option<PathBuf> {
    let name = match v { Variant::Horizon => "horizon", Variant::Bhh => "bhh" };
    let d = RESSOURCEN.get()?.join(name);
    d.join("stand.json").exists().then_some(d)
}

/// Kopiert die mitgelieferten Programme (und die Vorbereitungs-Dateien) in den Datenordner,
/// wenn dort noch nichts oder ein anderer Stand liegt. Ein selbst gebauter, neuerer Stand
/// aus dem Quellcode bleibt unberuehrt, solange er zum Quellcode passt.
pub fn install_bundled(v: Variant) -> Result<bool> {
    let Some(von) = mitgeliefert(v) else { return Ok(false) };
    let geliefert: Stand = crate::store::read(&von.join("stand.json")).unwrap_or_default();
    let jetzt: Stand = crate::store::read(&stand_file(v)?).unwrap_or_default();
    let eigener_bau = source_dir(v).is_some_and(|s| source_revision(v, &s) == jetzt.revision);
    if jetzt.revision == geliefert.revision || eigener_bau {
        return Ok(false);
    }
    fn kopieren(von: &Path, nach: &Path) -> Result<()> {
        paths::ensure_dir(nach)?;
        for e in std::fs::read_dir(von)?.flatten() {
            let ziel = nach.join(e.file_name());
            if e.path().is_dir() {
                kopieren(&e.path(), &ziel)?;
            } else {
                let tmp = ziel.with_extension("neu");
                std::fs::copy(e.path(), &tmp)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let ausfuehrbar = e.path().extension().is_none();
                    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(if ausfuehrbar { 0o755 } else { 0o644 }))?;
                }
                std::fs::rename(&tmp, &ziel)?;
            }
        }
        Ok(())
    }
    kopieren(&von.join("bin"), &v.bin_dir()?)?;
    let pr = v.root()?.join("prepare-root");
    if pr.exists() {
        std::fs::remove_dir_all(&pr)?;
    }
    kopieren(&von.join("prepare-root"), &pr)?;
    crate::store::write(&stand_file(v)?, &geliefert)?;
    Ok(true)
}

/// Der `--root` fuer `horizon-prepare`: der Quellcode (Entwicklung) oder die mitgelieferten
/// Vorbereitungs-Dateien im Datenordner (research-lock.json + Exporter-Quellen; beschreibbar,
/// weil horizon-prepare dort seinen `.local/`-Zwischenstand ablegt).
pub fn prepare_root(v: Variant) -> Option<PathBuf> {
    if let Some(s) = source_dir(v) {
        return Some(s);
    }
    let _ = install_bundled(v);
    let pr = v.root().ok()?.join("prepare-root");
    pr.join("research-lock.json").exists().then_some(pr)
}

pub fn binary(v: Variant, name: &str) -> Result<PathBuf> {
    Ok(v.bin_dir()?.join(name))
}

fn programme(v: Variant) -> [&'static str; 2] {
    [v.client(), "horizon-prepare"]
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Stand {
    revision: String,
    gebaut_am: String,
}

fn stand_file(v: Variant) -> Result<PathBuf> {
    Ok(v.bin_dir()?.join("stand.json"))
}

#[derive(Debug, Clone, Serialize)]
pub struct RustStatus {
    pub variant: Variant,
    /// Der Quellcode liegt bei BuildMC.
    pub source_found: bool,
    pub source_revision: String,
    /// `cargo` ist auf dem Rechner vorhanden (zum Bauen noetig).
    pub cargo_found: bool,
    /// Alle Programme sind gebaut.
    pub built: bool,
    /// Gebaut, aber aus einem aelteren Quellcode-Stand.
    pub outdated: bool,
    /// Version des gebauten Spiels (`--launch-api`).
    pub horizon_version: String,
    pub launch_api: String,
    /// Spieldaten vorbereitet und aktuell (`horizon-prepare --check`).
    pub prepared: bool,
    /// Ein Microsoft-Konto ist angemeldet (fuer Vorbereiten und Spielen noetig).
    pub microsoft_account: bool,
    pub account_name: String,
    pub minecraft_version: String,
    pub building: bool,
    pub preparing: bool,
    /// Nur BHH: JDK 25 fuer die eingebettete JVM gefunden.
    pub jdk_found: bool,
    /// Fertig mitgeliefert (Release): kein Bauen noetig.
    pub bundled: bool,
}

pub fn cargo_found() -> bool {
    Command::new("cargo").arg("--version").stdout(Stdio::null()).stderr(Stdio::null())
        .status().map(|s| s.success()).unwrap_or(false)
}

/// Ein JDK 25 (mit javac und libjvm), wie BHH es zum Bauen und Starten braucht:
/// erst die von BuildMC verwalteten, dann `JAVA_HOME`.
pub fn jdk25() -> Option<PathBuf> {
    let gut = |h: &Path| h.join("bin/javac").exists() && h.join("lib/server/libjvm.so").exists()
        && std::fs::read_to_string(h.join("release")).map(|r| r.contains("JAVA_VERSION=\"25")).unwrap_or(false);
    let mut kandidaten: Vec<PathBuf> = paths::jdks().ok()
        .and_then(|d| std::fs::read_dir(d).ok())
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    if let Some(j) = std::env::var_os("JAVA_HOME") {
        kandidaten.push(PathBuf::from(j));
    }
    kandidaten.into_iter().find(|h| gut(h))
}

/// Was `<spiel> --launch-api` meldet: Version, API-Stand, erlaubte Optionen.
pub fn launch_api(v: Variant) -> Option<serde_json::Value> {
    let bin = binary(v, v.client()).ok()?;
    if !bin.exists() {
        return None;
    }
    // BHH ohne Mods startet keine JVM: fuer --launch-api reicht ein leerer Mod-Ordner.
    let out = Command::new(bin).arg("--launch-api").env("BHH_MODS_DIR", "/nonexistent")
        .stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Ob das gebaute Spiel eine Option kennt -- unbekannte lehnt Horizon mit Code 2 ab.
pub fn supports_option(api: &serde_json::Value, name: &str) -> bool {
    api["options"].as_array().map(|o| o.iter().any(|x| x["name"] == name)).unwrap_or(false)
}

pub fn status(v: Variant) -> Result<RustStatus> {
    let _ = install_bundled(v);
    let src = source_dir(v);
    let revision = src.as_deref().map(|s| source_revision(v, s)).unwrap_or_default();
    let mut built = programme(v).iter().all(|p| binary(v, p).map(|b| b.exists()).unwrap_or(false));
    if v == Variant::Bhh {
        built &= binary(v, "bhh-api.jar").map(|b| b.exists()).unwrap_or(false);
    }
    let stand: Stand = stand_file(v).ok().and_then(|f| crate::store::read(&f).ok()).unwrap_or_default();
    let api = if built { launch_api(v) } else { None };
    let accounts = crate::store::load_accounts();
    let konto = accounts.iter().find(|a| a.active && !a.is_offline())
        .or_else(|| accounts.iter().find(|a| !a.is_offline()));
    Ok(RustStatus {
        variant: v,
        source_found: src.is_some(),
        source_revision: revision.clone(),
        cargo_found: cargo_found(),
        built,
        outdated: built && src.is_some() && stand.revision != revision,
        bundled: mitgeliefert(v).is_some(),
        horizon_version: api.as_ref().and_then(|a| a["horizon"].as_str()).unwrap_or("").to_string(),
        launch_api: api.as_ref().and_then(|a| a["launch_api"].as_str()).unwrap_or("").to_string(),
        prepared: built && super::prepare::check(v).unwrap_or(false),
        microsoft_account: konto.is_some(),
        account_name: konto.map(|a| a.username.clone()).unwrap_or_default(),
        minecraft_version: super::MINECRAFT_VERSION.into(),
        building: BAU.lock().map(|b| b[v.index()]).unwrap_or(false),
        preparing: super::prepare::running(v),
        // Fehlt es, laedt BuildMC es beim ersten BHH-Start selbst.
        jdk_found: v == Variant::Horizon || jdk25().is_some() || mitgeliefert(v).is_some(),
    })
}

static BAU: Mutex<[bool; 2]> = Mutex::new([false; 2]);

struct BauGuard(Variant);
impl Drop for BauGuard {
    fn drop(&mut self) {
        if let Ok(mut b) = BAU.lock() {
            b[self.0.index()] = false;
        }
    }
}

/// Fuehrt einen Bauschritt aus und reicht jede Zeile weiter.
fn laufen(mut cmd: Command, was: &str, zeile: &impl Fn(&str, f32), fertig: &mut f32, gesamt: f32) -> Result<()> {
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| Error::other(format!("{was} ließ sich nicht starten: {e}")))?;
    let stdout = child.stdout.take().expect("stdout umgeleitet");
    let raus = std::thread::spawn(move || BufReader::new(stdout).lines().map_while(std::result::Result::ok).collect::<Vec<_>>());
    let mut fehler = Vec::new();
    for line in BufReader::new(child.stderr.take().expect("stderr umgeleitet")).lines().map_while(std::result::Result::ok) {
        let t = line.trim();
        if t.starts_with("Compiling") {
            *fertig += 1.0;
        }
        if t.starts_with("error") || t.contains("Fehler") || t.contains("could not find system library") || t.contains("pkg-config") {
            fehler.push(t.to_string());
        }
        zeile(t, (*fertig / gesamt).min(0.99));
    }
    for l in raus.join().unwrap_or_default() {
        zeile(&l, (*fertig / gesamt).min(0.99));
    }
    let status = child.wait()?;
    if !status.success() {
        let hinweis = if fehler.iter().any(|f| f.contains("alsa")) { " -- es fehlt libasound2-dev" } else { "" };
        return Err(Error::other(format!("{was} fehlgeschlagen{hinweis}: {}",
            fehler.last().cloned().unwrap_or_else(|| format!("endete mit {status}")))));
    }
    Ok(())
}

/// Baut die Programme einer Variante aus ihrem Quellcode. `zeile` bekommt jede
/// Ausgabezeile und einen geschaetzten Anteil (0..1). Blockiert.
pub fn build(v: Variant, zeile: impl Fn(&str, f32)) -> Result<()> {
    {
        let mut b = BAU.lock().map_err(|_| Error::other("Bau-Sperre beschädigt"))?;
        if b[v.index()] {
            return Err(Error::other("Der Bau läuft schon"));
        }
        b[v.index()] = true;
    }
    let _guard = BauGuard(v);

    let src = source_dir(v).ok_or_else(|| Error::not_found(format!("{}-Quellcode nicht gefunden", v.label())))?;
    if !cargo_found() {
        return Err(Error::other("cargo fehlt -- Rust installieren: https://rustup.rs"));
    }
    let jdk = if v == Variant::Bhh {
        Some(jdk25().ok_or_else(|| Error::other("BHH braucht ein JDK 25 -- unter Java einmal Java 25 installieren"))?)
    } else {
        None
    };
    let target = v.build_dir()?;
    paths::ensure_dir(&target)?;
    let gesamt = std::fs::read_to_string(src.join("Cargo.lock"))
        .map(|t| t.matches("[[package]]").count()).unwrap_or(600).max(1) as f32;
    let mut fertig = 0f32;
    zeile(&format!("Baue {} ({}) -- beim ersten Mal 20 bis 40 Minuten", v.label(), &source_revision(v, &src)[..7.min(source_revision(v, &src).len())]), 0.0);

    // Ohne libudev (Linux) kein Controller-Support: dann ohne das Feature bauen statt abbrechen.
    let controller = !cfg!(target_os = "linux") || Command::new("pkg-config").args(["--exists", "libudev"])
        .status().map(|s| s.success()).unwrap_or(false);
    if !controller {
        zeile("Hinweis: libudev-dev fehlt -- ohne Controller-Unterstützung gebaut (sudo apt install libudev-dev, dann neu bauen)", 0.0);
    }

    if let Some(jdk) = &jdk {
        let mut cmd = Command::new("bash");
        cmd.arg(src.join("java/build.sh")).current_dir(src.join("java"))
            .env("PATH", format!("{}:{}", jdk.join("bin").display(), std::env::var("PATH").unwrap_or_default()));
        laufen(cmd, "Java-Seite (bhh-api.jar)", &zeile, &mut fertig, gesamt)?;
    }

    for paket in ["horizon-prepare", v.client()] {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(&src)
            .args(["build", "--release", "--message-format", "short", "-p", paket])
            .env("CARGO_TARGET_DIR", &target)
            .env("CARGO_TERM_COLOR", "never");
        if v == Variant::Horizon {
            cmd.arg("--locked");
        }
        if let Some(jdk) = &jdk {
            cmd.env("JAVA_HOME", jdk); // jni (invocation) sucht libjvm beim Bauen
        }
        if !controller && paket == v.client() && v == Variant::Horizon {
            cmd.args(["--no-default-features", "--features", "audio"]);
        }
        laufen(cmd, paket, &zeile, &mut fertig, gesamt)?;
    }

    let bin = v.bin_dir()?;
    paths::ensure_dir(&bin)?;
    let mut kopien: Vec<(PathBuf, &str)> = programme(v).iter().map(|p| (target.join("release").join(p), *p)).collect();
    if v == Variant::Bhh {
        kopien.push((src.join("java/build/bhh-api.jar"), "bhh-api.jar"));
    }
    for (von, name) in kopien {
        let tmp = bin.join(format!("{name}.neu"));
        std::fs::copy(&von, &tmp)?;
        std::fs::rename(&tmp, bin.join(name))?;
    }
    if v == Variant::Bhh {
        // Logging-APIs der Java-Seite (slf4j, log4j) neben bhh-api.jar.
        let lib = bin.join("lib");
        paths::ensure_dir(&lib)?;
        for e in std::fs::read_dir(src.join("java/build/lib"))?.flatten() {
            std::fs::copy(e.path(), lib.join(e.file_name()))?;
        }
    }
    crate::store::write(&stand_file(v)?, &Stand { revision: source_revision(v, &src), gebaut_am: super::now() })?;
    zeile(&format!("{} ist gebaut", v.label()), 1.0);
    Ok(())
}
