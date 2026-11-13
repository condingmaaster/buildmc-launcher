//! Spieldaten fuer die Rust-Edition vorbereiten (`horizon-prepare`).
//!
//! Horizon enthaelt keine Minecraft-Dateien. Es erzeugt seine Spieldaten aus
//! der eigenen 26.3 des Spielers -- die laedt BuildMC ohnehin fuer Java:
//! Versions-JSON, Client-Jar, Bibliotheken, Assets, dazu ein JDK 25 und die
//! Server-Jar. Das alles nur fuer ein Microsoft-Konto, dem Java Edition
//! gehoert -- dieselbe Besitzer-Pruefung wie Horizons eigener Launcher.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::auth::Account;
use crate::download::{Progress, Sink};
use crate::error::{Error, Result};
use crate::paths;

use super::{engine, Variant, MINECRAFT_VERSION};

static LAEUFT: [AtomicBool; 2] = [AtomicBool::new(false), AtomicBool::new(false)];

pub fn running(v: Variant) -> bool {
    LAEUFT[v.index()].load(Ordering::SeqCst)
}

struct Guard(Variant);
impl Drop for Guard {
    fn drop(&mut self) {
        LAEUFT[self.0.index()].store(false, Ordering::SeqCst);
    }
}

/// Das aktive Microsoft-Konto, frisch erneuert und gespeichert. Offline-Konten
/// zaehlen nicht: Horizon startet nur fuer jemanden, dem Java Edition gehoert.
pub async fn microsoft_account() -> Result<Account> {
    let accounts = crate::store::load_accounts();
    let picked = accounts.iter().find(|a| a.active && !a.is_offline())
        .or_else(|| accounts.iter().find(|a| !a.is_offline()))
        .cloned()
        .ok_or_else(|| Error::other("Für die Rust-Edition wird ein Microsoft-Konto gebraucht, dem Minecraft: Java Edition gehört"))?;
    let account = crate::auth::microsoft::ensure_fresh(&picked).await.unwrap_or_else(|_| picked.clone());
    if account.access_token != picked.access_token {
        crate::store::with_accounts(|all| {
            for a in all.iter_mut() {
                if a.id == account.id {
                    *a = account.clone();
                }
            }
            Ok(())
        })?;
    }
    Ok(account)
}

/// Gehoert dem Konto Java Edition? Das Minecraft-Profil gibt es nur dann.
async fn besitzt_java(account: &Account) -> Result<()> {
    crate::auth::profile::fetch_own(&account.access_token).await.map(|_| ()).map_err(|e| {
        Error::other(format!("Das Konto {} besitzt laut Mojang kein Minecraft: Java Edition ({e})", account.username))
    })
}

fn melden(sink: &Arc<dyn Sink>, label: &str, detail: &str, anteil: f32, done: bool, error: Option<String>) {
    sink.emit(&Progress {
        id: "rust-prepare".into(),
        label: label.into(),
        detail: detail.into(),
        current: (anteil.clamp(0.0, 1.0) * 1000.0) as u64,
        total: 1000,
        bytes_per_second: 0,
        done,
        error,
    });
}

/// Sind die Spieldaten vollstaendig und aktuell? (`horizon-prepare --check`)
pub fn check(v: Variant) -> Result<bool> {
    let bin = engine::binary(v, "horizon-prepare")?;
    let Some(src) = engine::prepare_root(v) else { return Ok(false) };
    if !bin.exists() {
        return Ok(false);
    }
    let out = v.prepared_dir()?;
    if !out.exists() {
        return Ok(false);
    }
    let status = Command::new(bin)
        .current_dir(&src)
        .arg("--root").arg(&src)
        .arg("--check").arg("--output").arg(&out)
        .stdout(Stdio::null()).stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

/// Server-Jar der Version laden (steht in der Versions-JSON, kein Launcher
/// behaelt sie). Nur nach der Besitzer-Pruefung aufrufen. Mit Fortschritt und ohne
/// Gesamt-Zeitlimit: ueber ein langsames Netz dauern die ~60 MB Minuten.
async fn server_jar(sink: &Arc<dyn Sink>) -> Result<std::path::PathBuf> {
    use futures::StreamExt;
    // Geteilt: Horizon und BHH brauchen dieselbe 26.3-Server-Jar.
    let ziel = paths::root()?.join(format!("server-{MINECRAFT_VERSION}.jar"));
    let json: serde_json::Value = crate::store::read(&paths::version_json(MINECRAFT_VERSION)?)?;
    let url = json["downloads"]["server"]["url"].as_str()
        .ok_or_else(|| Error::other("Versions-JSON ohne Server-Download"))?;
    let sha1 = json["downloads"]["server"]["sha1"].as_str().unwrap_or("");
    let groesse = json["downloads"]["server"]["size"].as_u64().unwrap_or(0);
    if ziel.exists() && (sha1.is_empty() || crate::download::file_sha1(&ziel).ok().as_deref() == Some(sha1)) {
        return Ok(ziel);
    }
    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(15))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| Error::other(e.to_string()))?;
    let antwort = client.get(url).send().await.map_err(|e| Error::other(format!("Server-Jar: {e}")))?
        .error_for_status().map_err(|e| Error::other(format!("Server-Jar: {e}")))?;
    let gesamt = antwort.content_length().unwrap_or(groesse).max(1);
    let tmp = ziel.with_extension("jar.part");
    paths::ensure_parent(&tmp)?;
    let mut datei = std::fs::File::create(&tmp)?;
    let mut geladen = 0u64;
    let mut gemeldet = 0u64;
    let beginn = std::time::Instant::now();
    let mut strom = antwort.bytes_stream();
    while let Some(stueck) = strom.next().await {
        let stueck = stueck.map_err(|e| Error::other(format!("Server-Jar abgebrochen: {e}")))?;
        std::io::Write::write_all(&mut datei, &stueck)?;
        geladen += stueck.len() as u64;
        if geladen - gemeldet > 512 * 1024 || geladen == gesamt {
            gemeldet = geladen;
            let mbs = geladen as f64 / 1_048_576.0 / beginn.elapsed().as_secs_f64().max(0.1);
            melden(sink, "Server-Jar wird geladen", &format!("{:.1} von {:.1} MB · {:.1} MB/s",
                geladen as f64 / 1_048_576.0, gesamt as f64 / 1_048_576.0, mbs),
                0.12 + 0.03 * (geladen as f32 / gesamt as f32), false, None);
        }
    }
    drop(datei);
    if !sha1.is_empty() && crate::download::file_sha1(&tmp)? != sha1 {
        let _ = std::fs::remove_file(&tmp);
        return Err(Error::other("Server-Jar: Prüfsumme stimmt nicht"));
    }
    std::fs::rename(&tmp, &ziel)?;
    Ok(ziel)
}

/// Alles vorbereiten: Konto pruefen, 26.3 und Java 25 bereitstellen,
/// Server-Jar laden, `horizon-prepare --prepare --complete` ausfuehren.
pub async fn prepare(v: Variant, sink: Arc<dyn Sink>) -> Result<()> {
    if LAEUFT[v.index()].swap(true, Ordering::SeqCst) {
        return Err(Error::other("Die Vorbereitung läuft schon"));
    }
    let _guard = Guard(v);
    let ergebnis = prepare_inner(v, &sink).await;
    if let Err(e) = &ergebnis {
        melden(&sink, "Rust-Edition vorbereiten", "", 1.0, true, Some(e.to_string()));
    }
    ergebnis
}

async fn prepare_inner(v: Variant, sink: &Arc<dyn Sink>) -> Result<()> {
    let bin = engine::binary(v, "horizon-prepare")?;
    if !bin.exists() {
        return Err(Error::other(format!("{} ist noch nicht gebaut", v.label())));
    }
    let src = engine::prepare_root(v).ok_or_else(|| Error::not_found(format!("{}: Vorbereitungs-Dateien fehlen", v.label())))?;

    melden(sink, "Konto wird geprüft", "", 0.01, false, None);
    let account = microsoft_account().await?;
    besitzt_java(&account).await?;

    melden(sink, &format!("Minecraft {MINECRAFT_VERSION} wird geladen"), "Versionsliste bei Mojang abfragen …", 0.03, false, None);
    crate::launch::install::ensure(MINECRAFT_VERSION, 8, false, sink.clone()).await?;

    melden(sink, "Java 25 wird bereitgestellt", "", 0.10, false, None);
    // horizon-prepare uebersetzt kleine Exporter mit javac: es muss ein JDK sein, keine reine JRE.
    let home_von = |j: &crate::java::JavaInfo| std::path::Path::new(&j.path).parent().and_then(|p| p.parent()).map(|p| p.to_path_buf());
    let jdk = |h: &std::path::Path| h.join("bin").join(if cfg!(windows) { "javac.exe" } else { "javac" }).exists();
    let java_home = match crate::java::pick(25, None).ok().filter(|j| j.major == 25).and_then(|j| home_von(&j)).filter(|h| jdk(h)) {
        Some(h) => h,
        None => {
            let j = crate::java::install::install(25).await?;
            home_von(&j).filter(|h| jdk(h)).ok_or_else(|| Error::other("Java 25 ohne javac -- ein JDK wird gebraucht"))?
        }
    };

    melden(sink, "Server-Jar wird geladen", "", 0.12, false, None);
    let server_jar = server_jar(sink).await?;

    let out = v.prepared_dir()?;
    paths::ensure_dir(&out)?;
    let args: Vec<std::ffi::OsString> = vec![
        "--root".into(), src.clone().into(),
        "--prepare".into(), "--complete".into(),
        "--progress".into(), "json".into(),
        "--output".into(), out.into(),
        "--version-json".into(), paths::version_json(MINECRAFT_VERSION)?.into(),
        "--client-jar".into(), paths::version_jar(MINECRAFT_VERSION)?.into(),
        "--libraries-dir".into(), paths::libraries()?.into(),
        "--assets-dir".into(), paths::assets()?.into(),
        "--java-home".into(), java_home.into(),
        "--server-jar".into(), server_jar.into(),
    ];

    let sink2 = sink.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let mut child = Command::new(&bin)
            .current_dir(&src)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Error::other(format!("horizon-prepare ließ sich nicht starten: {e}")))?;
        let stderr = child.stderr.take().expect("stderr umgeleitet");
        let fehlertext = std::thread::spawn(move || {
            BufReader::new(stderr).lines().map_while(std::result::Result::ok).collect::<Vec<_>>()
        });
        let mut anteil = 0.0f32;
        let mut text = String::from("Spieldaten werden erzeugt");
        let mut fehler: Option<String> = None;
        for line in BufReader::new(child.stdout.take().expect("stdout umgeleitet")).lines().map_while(std::result::Result::ok) {
            let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else { continue };
            if let Some(f) = v["fraction"].as_f64() {
                anteil = f as f32;
            }
            if let Some(m) = v["message"].as_str() {
                text = m.to_string();
            }
            if let Some(e) = v["error"].as_str() {
                fehler = Some(e.to_string());
            }
            // Die eigentliche Vorbereitung ist der groesste Teil: 15 % .. 100 %.
            melden(&sink2, "Spieldaten werden erzeugt", &text, 0.15 + anteil * 0.85, false, None);
        }
        let status = child.wait()?;
        let stderr_zeilen = fehlertext.join().unwrap_or_default();
        if !status.success() {
            let grund = fehler.or_else(|| stderr_zeilen.iter().rev().find(|l| !l.trim().is_empty()).cloned())
                .unwrap_or_else(|| format!("horizon-prepare endete mit {status}"));
            return Err(Error::other(format!("Vorbereitung fehlgeschlagen: {grund}")));
        }
        Ok(())
    }).await.map_err(|e| Error::other(format!("Vorbereitung abgebrochen: {e}")))??;

    melden(sink, &format!("{} ist bereit", v.label()), "", 1.0, true, None);
    Ok(())
}
