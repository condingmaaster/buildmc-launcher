//! Rust-Edition starten und stoppen -- ueber Horizons Launch-API.
//!
//! `horizon-client --resources PREPARED --game-dir INSTANZ/game --home INSTANZ/home --session-stdin`
//! Das Konto geht als Session-JSON ueber stdin hinein und wird danach
//! geschlossen -- es landet weder auf der Kommandozeile noch auf der Platte.
//! Exit-Codes: 0 beendet, 1/101 Absturz (Bericht-Pfad in der letzten
//! stderr-Zeile), 2 abgelehnte Kommandozeile.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};

use serde::Serialize;

use crate::error::{Error, Result};
use crate::paths;

use super::{engine, Variant};

const CRASH_ZEILE: &str = "Horizon: crash report saved to ";

#[derive(Debug, Clone, Serialize)]
pub struct RunningRustInstance {
    pub instance_id: String,
    pub pid: u32,
    pub started_at: String,
}

struct Entry {
    child: Arc<Mutex<Child>>,
    pid: u32,
    started_at: String,
}

fn registry() -> &'static Mutex<HashMap<String, Entry>> {
    static R: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn is_running(instance_id: &str) -> bool {
    registry().lock().map(|r| r.contains_key(instance_id)).unwrap_or(false)
}

pub fn running() -> Vec<RunningRustInstance> {
    let Ok(reg) = registry().lock() else { return Vec::new() };
    reg.iter().map(|(id, e)| RunningRustInstance {
        instance_id: id.clone(), pid: e.pid, started_at: e.started_at.clone(),
    }).collect()
}

/// Rueckmeldungen an die Oberflaeche.
pub trait RustEvents: Send + Sync + 'static {
    fn log_line(&self, instance_id: &str, line: &str);
    fn stopped(&self, instance_id: &str, exit_code: i32, played_seconds: u64, crash_report: Option<String>);
}

/// Startet eine Instanz mit dem aktiven Microsoft-Konto.
pub async fn start(v: Variant, instance_id: &str, events: Arc<dyn RustEvents>) -> Result<RunningRustInstance> {
    if is_running(instance_id) {
        return Err(Error::other("Die Instanz läuft schon"));
    }
    let bin = engine::binary(v, v.client())?;
    if !bin.exists() {
        return Err(Error::other(format!("{} ist noch nicht gebaut", v.label())));
    }
    if !super::prepare::check(v)? {
        return Err(Error::other("Die Spieldaten sind nicht vorbereitet oder veraltet -- zuerst „Vorbereiten“"));
    }
    let api = engine::launch_api(v).ok_or_else(|| Error::other(format!("{} antwortet nicht auf --launch-api", v.client())))?;

    let mut instance = super::load(v, instance_id)?;
    let account = super::prepare::microsoft_account().await?;
    let session = serde_json::json!({
        "name": account.username,
        "uuid": account.uuid.replace('-', ""),
        "access_token": account.access_token,
        "user_type": "msa",
    })
    .to_string();

    let game = instance.game_dir()?;
    let home = instance.home_dir()?;
    paths::ensure_dir(&game)?;
    paths::ensure_dir(&home)?;

    let mut cmd = Command::new(&bin);
    cmd.current_dir(&game)
        .arg("--resources").arg(v.prepared_dir()?)
        .arg("--game-dir").arg(&game)
        .arg("--home").arg(&home)
        .arg("--session-stdin");
    // Nur Optionen, die dieser Bau kennt -- unbekannte lehnt Horizon mit Code 2 ab.
    if !instance.quick_server.is_empty() && engine::supports_option(&api, "--quickPlayMultiplayer") {
        cmd.arg("--quickPlayMultiplayer").arg(&instance.quick_server);
    } else if !instance.quick_world.is_empty() && engine::supports_option(&api, "--quickPlaySingleplayer") {
        cmd.arg("--quickPlaySingleplayer").arg(&instance.quick_world);
    }
    if instance.fullscreen && engine::supports_option(&api, "--fullscreen") {
        cmd.arg("--fullscreen");
    }
    if v == Variant::Bhh {
        // Fabric-Bruecke: Mods der Instanz, Java-Seite, Heap; libjvm aus dem verwalteten JDK 25.
        let jdk = match engine::jdk25() {
            Some(j) => j,
            None => {
                // Einmalig: BuildMC laedt das JDK 25 selbst (wie fuer Java-Instanzen).
                crate::java::install::install(25).await?;
                engine::jdk25().ok_or_else(|| Error::other("JDK 25 ließ sich nicht einrichten"))?
            }
        };
        let mods = instance.fabric_mods_dir()?;
        paths::ensure_dir(&mods)?;
        let ld = std::env::var("LD_LIBRARY_PATH").unwrap_or_default();
        // Die Bibliotheken der eigenen 26.3 (Guava, Gson, DataFixerUpper, Brigadier ...) --
        // dieselben Dateien, die das Vorbereiten geladen hat; BHH liefert sie nicht selbst aus.
        let info = crate::manifest::read_local(super::MINECRAFT_VERSION)?;
        let libs: Vec<String> = crate::download::plan::libraries(&info, &crate::manifest::rules::RuleContext::current())?
            .into_iter().filter(|(item, native)| !native && item.target.exists())
            .map(|(item, _)| item.target.display().to_string()).collect();
        cmd.env("BHH_LIBRARIES", libs.join(":"));
        cmd.env("BHH_API_JAR", engine::binary(v, "bhh-api.jar")?)
            .env("BHH_MODS_DIR", &mods)
            .env("BHH_HEAP_MB", instance.jvm_heap_mb.to_string())
            .env("JAVA_HOME", &jdk)
            .env("LD_LIBRARY_PATH", format!("{}{}{}", jdk.join("lib/server").display(), if ld.is_empty() { "" } else { ":" }, ld));
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| Error::other(format!("horizon-client ließ sich nicht starten: {e}")))?;
    {
        let mut stdin = child.stdin.take().expect("stdin umgeleitet");
        stdin.write_all(session.as_bytes())?;
        // stdin schliessen: Horizon liest bis zum Ende.
    }
    drop(session);

    let pid = child.id();
    let started_at = super::now();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let child = Arc::new(Mutex::new(child));
    registry().lock().map_err(|_| Error::other("Prozessliste beschädigt"))?.insert(
        instance_id.to_string(),
        Entry { child: child.clone(), pid, started_at: started_at.clone() },
    );

    instance.last_played = started_at.clone();
    instance.launch_count += 1;
    super::save(&instance)?;

    // Ausgaben mitschreiben und weiterreichen.
    let log_file = home.join("buildmc-launch.log");
    let _ = std::fs::write(&log_file, "");
    let crash = Arc::new(Mutex::new(None::<String>));
    let mut leser = Vec::new();
    for strom in [stdout.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
                  stderr.map(|s| Box::new(s) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let (id, ev, file, crash) = (instance_id.to_string(), events.clone(), log_file.clone(), crash.clone());
        leser.push(std::thread::spawn(move || {
            for line in BufReader::new(strom).lines().map_while(std::result::Result::ok) {
                if let Some(pfad) = line.strip_prefix(CRASH_ZEILE) {
                    if let Ok(mut c) = crash.lock() {
                        *c = Some(pfad.trim().to_string());
                    }
                }
                if let Ok(mut f) = std::fs::OpenOptions::new().append(true).open(&file) {
                    let _ = writeln!(f, "{line}");
                }
                ev.log_line(&id, &line);
            }
        }));
    }

    let id = instance_id.to_string();
    let beginn = std::time::Instant::now();
    std::thread::spawn(move || {
        let code = loop {
            let fertig = child.lock().ok().and_then(|mut c| c.try_wait().ok().flatten());
            if let Some(s) = fertig {
                break s.code().unwrap_or(-1);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        };
        for l in leser {
            let _ = l.join();
        }
        if let Ok(mut r) = registry().lock() {
            r.remove(&id);
        }
        let sekunden = beginn.elapsed().as_secs();
        super::record_playtime(v, &id, sekunden);
        let bericht = crash.lock().ok().and_then(|c| c.clone());
        events.stopped(&id, code, sekunden, bericht);
    });

    Ok(RunningRustInstance { instance_id: instance_id.to_string(), pid, started_at })
}

pub fn stop(instance_id: &str) -> Result<()> {
    let reg = registry().lock().map_err(|_| Error::other("Prozessliste beschädigt"))?;
    let e = reg.get(instance_id).ok_or_else(|| Error::not_found("Die Instanz läuft nicht"))?;
    e.child.lock().map_err(|_| Error::other("Prozess gesperrt"))?.kill()?;
    Ok(())
}
