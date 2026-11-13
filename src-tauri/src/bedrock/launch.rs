//! Bedrock starten/stoppen -- strukturgleich zu `launch::process`, aber
//! bewusst eine eigene, getrennte Registrierung.
//!
//! `launch::process::Events::stopped()` ruft fest Javas
//! `record_playtime`/`analyze_instance` auf -- eine gemeinsame Nutzung ohne
//! Umbau wuerde Java-Absturzregeln auf Bedrock-Protokolle anwenden.
//! Zusammenlegen ist eine spaetere, eigene Aufraeumarbeit (siehe Plan).

use std::collections::{HashMap, HashSet};
use std::process::{Child, Stdio};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use super::detect::Launcher;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct RunningBedrockInstance {
    pub instance_id: String,
    pub pid: u32,
    pub started_at: String,
}

struct Entry {
    child: std::sync::Arc<Mutex<Child>>,
    pid: u32,
    started_at: String,
}

fn registry() -> &'static Mutex<HashMap<String, Entry>> {
    static R: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

fn starting() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

struct StartingGuard<'a>(&'a str);
impl Drop for StartingGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut s) = starting().lock() {
            s.remove(self.0);
        }
    }
}

pub fn is_running(instance_id: &str) -> bool {
    registry().lock().map(|r| r.contains_key(instance_id)).unwrap_or(false)
        || starting().lock().map(|s| s.contains(instance_id)).unwrap_or(false)
}

pub fn running() -> Vec<RunningBedrockInstance> {
    let Ok(reg) = registry().lock() else { return Vec::new() };
    reg.iter()
        .map(|(id, e)| RunningBedrockInstance {
            instance_id: id.clone(),
            pid: e.pid,
            started_at: e.started_at.clone(),
        })
        .collect()
}

/// Rueckruf bei Ereignissen -- eigenes, kleines Gegenstueck zu
/// `launch::process::Events`, ohne dessen Java-spezifische Anschluesse.
pub trait BedrockEvents: Send + Sync + 'static {
    fn stopped(&self, instance_id: &str, exit_code: i32, played_seconds: u64);
}

pub struct NoBedrockEvents;
impl BedrockEvents for NoBedrockEvents {
    fn stopped(&self, _: &str, _: i32, _: u64) {}
}

/// Startet `mcpelauncher-client` fuer eine Instanz.
pub fn start(
    instance_id: &str,
    client: &Launcher,
    game_dir: &std::path::Path,
    data_dir: &std::path::Path,
    cache_dir: &std::path::Path,
    events: std::sync::Arc<dyn BedrockEvents>,
) -> Result<RunningBedrockInstance> {
    // Pruefen und reservieren als eine Einheit -- gleiches Muster wie beim
    // Java-Doppelstart-Schutz, aus demselben Grund: zwei fast gleichzeitige
    // Starts sollen nicht beide die Pruefung bestehen, bevor einer von
    // ihnen eingetragen ist.
    {
        let reg = registry().lock().map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?;
        let mut starting_set = starting().lock().map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?;
        if reg.contains_key(instance_id) || starting_set.contains(instance_id) {
            return Err(Error::AlreadyRunning);
        }
        starting_set.insert(instance_id.to_string());
    }
    let _guard = StartingGuard(instance_id);

    let child = client.command("mcpelauncher-client")
        .args([
            "--game-dir", &game_dir.display().to_string(),
            "--data-dir", &data_dir.display().to_string(),
            "--cache-dir", &cache_dir.display().to_string(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| Error::other(format!("mcpelauncher-client ließ sich nicht starten: {e}")))?;

    let pid = child.id();
    let started_at = super::now();
    let shared = std::sync::Arc::new(Mutex::new(child));
    let started_instant = std::time::Instant::now();

    registry()
        .lock()
        .map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?
        .insert(
            instance_id.to_string(),
            Entry { child: shared.clone(), pid, started_at: started_at.clone() },
        );

    {
        let id = instance_id.to_string();
        std::thread::spawn(move || {
            let status = shared.lock().map(|mut c| c.wait()).ok();
            let played = started_instant.elapsed().as_secs();

            if let Ok(mut reg) = registry().lock() {
                reg.remove(&id);
            }

            let code = status.and_then(|s| s.ok()).and_then(|s| s.code()).unwrap_or(-1);
            super::record_playtime(&id, played);
            events.stopped(&id, code, played);
        });
    }

    Ok(RunningBedrockInstance { instance_id: instance_id.to_string(), pid, started_at })
}

pub fn stop(instance_id: &str) -> Result<()> {
    let reg = registry()
        .lock()
        .map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?;

    let Some(entry) = reg.get(instance_id) else {
        return Err(Error::not_found("Diese Instanz läuft nicht"));
    };
    let child = entry.child.clone();
    drop(reg);

    child
        .lock()
        .map_err(|_| Error::other("Der Prozess ist blockiert"))?
        .kill()
        .map_err(|e| Error::other(format!("Beenden fehlgeschlagen: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nicht_laufende_instanz_laesst_sich_nicht_beenden() {
        assert!(stop("gibt-es-nicht").is_err());
        assert!(!is_running("gibt-es-nicht"));
    }
}
