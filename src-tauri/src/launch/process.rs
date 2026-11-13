//! Laufende Spiele verwalten.
//!
//! Der Launcher haelt zu jeder laufenden Instanz den Kindprozess, liest
//! dessen Ausgabe mit und meldet das Ende. Bewusst ohne Tauri-Abhaengigkeit:
//! die Ereignisse gehen ueber einen Rueckruf hinaus, damit das Modul
//! ohne laufende Oberflaeche pruefbar bleibt.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::{Mutex, OnceLock};

use serde::Serialize;

use crate::error::{Error, Result};

/// Wie viele Protokollzeilen je Instanz im Speicher bleiben.
///
/// Minecraft schreibt bei vielen Mods Zehntausende Zeilen. Fuer die
/// Anzeige reicht das Ende, und die vollstaendige Ausgabe liegt ohnehin
/// in der Protokolldatei.
const LOG_CAPACITY: usize = 5000;

#[derive(Debug, Clone, Serialize)]
pub struct RunningInstance {
    pub instance_id: String,
    pub pid: u32,
    pub started_at: String,
}

struct Entry {
    /// Geteilt mit dem wartenden Faden.
    ///
    /// Der Kindprozess liegt hinter einer eigenen Sperre, damit auf sein
    /// Ende gewartet werden kann, **ohne** die Sperre der Liste zu halten --
    /// und ohne ihn vorher aus der Liste zu nehmen. Genau das war der
    /// Fehler: die Instanz galt dadurch sofort als beendet, wurde nicht
    /// als laufend angezeigt, und die Telemetrie fand keinen Prozess.
    child: std::sync::Arc<Mutex<Child>>,
    pid: u32,
    started_at: String,
}

fn registry() -> &'static Mutex<HashMap<String, Entry>> {
    static R: OnceLock<Mutex<HashMap<String, Entry>>> = OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Instanzen, deren Prozess gerade gestartet wird, aber noch nicht in der
/// Registrierung steht.
///
/// Ohne das laege zwischen der `is_running`-Pruefung in `spawn()` und dem
/// tatsaechlichen Eintragen eine Luecke: zwei fast gleichzeitige Starts
/// (z.B. ein Doppelklick, bevor die Oberflaeche reagiert) koennten beide
/// die Pruefung bestehen und denselben Java-Prozess zweimal starten.
fn starting() -> &'static Mutex<HashSet<String>> {
    static S: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Entfernt die Reservierung zuverlaessig, auch bei einem fruehen Fehler
/// (Java liess sich nicht starten) -- sonst gaelte die Instanz danach
/// faelschlich fuer immer als "wird gerade gestartet".
struct StartingGuard<'a>(&'a str);
impl Drop for StartingGuard<'_> {
    fn drop(&mut self) {
        if let Ok(mut s) = starting().lock() {
            s.remove(self.0);
        }
    }
}

fn logs() -> &'static Mutex<HashMap<String, VecDeque<String>>> {
    static L: OnceLock<Mutex<HashMap<String, VecDeque<String>>>> = OnceLock::new();
    L.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Meldungen an die Oberflaeche.
pub trait Events: Send + Sync + 'static {
    fn log_line(&self, instance_id: &str, line: &str);
    fn stopped(&self, instance_id: &str, exit_code: i32, played_seconds: u64);
}

/// Verwirft alles. Fuer Aufrufe ohne Oberflaeche.
pub struct NoEvents;
impl Events for NoEvents {
    fn log_line(&self, _: &str, _: &str) {}
    fn stopped(&self, _: &str, _: i32, _: u64) {}
}

pub fn is_running(instance_id: &str) -> bool {
    registry().lock().map(|r| r.contains_key(instance_id)).unwrap_or(false)
        || starting().lock().map(|s| s.contains(instance_id)).unwrap_or(false)
}

pub fn running() -> Vec<RunningInstance> {
    let Ok(reg) = registry().lock() else { return Vec::new() };
    reg.iter()
        .map(|(id, e)| RunningInstance {
            instance_id: id.clone(),
            pid: e.pid,
            started_at: e.started_at.clone(),
        })
        .collect()
}

pub fn log_of(instance_id: &str) -> Vec<String> {
    logs()
        .lock()
        .ok()
        .and_then(|l| l.get(instance_id).map(|d| d.iter().cloned().collect()))
        .unwrap_or_default()
}

pub fn clear_log(instance_id: &str) {
    if let Ok(mut l) = logs().lock() {
        l.remove(instance_id);
    }
}

/// Schreibt eine Zeile des Launchers in das Protokoll der Instanz.
///
/// Ohne das bleibt die Konsole leer, bis das Spiel selbst etwas ausgibt --
/// und das dauert: erst werden Dateien geprueft, Java gewaehlt und der
/// Prozess vorbereitet. In dieser Zeit sah es aus, als passiere nichts.
pub fn note(instance_id: &str, line: &str, events: &std::sync::Arc<dyn Events>) {
    let text = format!("[Launcher] {line}");
    push_line(instance_id, text.clone());
    events.log_line(instance_id, &text);
}

fn push_line(instance_id: &str, line: String) {
    if let Ok(mut l) = logs().lock() {
        let buffer = l.entry(instance_id.to_string()).or_default();
        if buffer.len() >= LOG_CAPACITY {
            buffer.pop_front();
        }
        buffer.push_back(line);
    }
}

/// Startet das Spiel.
///
/// Gibt zurueck, sobald der Prozess laeuft -- nicht erst, wenn das Spiel
/// fertig geladen hat. Das kann Minuten dauern, und die Oberflaeche soll
/// derweil das Protokoll zeigen.
pub fn spawn(
    instance_id: &str,
    java: &std::path::Path,
    arguments: &[String],
    working_dir: &std::path::Path,
    events: std::sync::Arc<dyn Events>,
) -> Result<RunningInstance> {
    // Pruefen und reservieren als eine Einheit -- sonst koennten zwei
    // gleichzeitige Aufrufe beide die Pruefung bestehen, bevor einer von
    // ihnen eingetragen ist.
    {
        let reg = registry().lock().map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?;
        let mut starting_set = starting().lock().map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?;
        if reg.contains_key(instance_id) || starting_set.contains(instance_id) {
            return Err(Error::AlreadyRunning);
        }
        starting_set.insert(instance_id.to_string());
    }
    // Reservierung wird spaetestens hier wieder entfernt, egal ob der
    // Start gelingt (dann steht die Instanz schon in der Registrierung)
    // oder scheitert.
    let _guard = StartingGuard(instance_id);

    let mut child = Command::new(java)
        .args(arguments)
        .current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|e| Error::other(format!("Java ließ sich nicht starten: {e}")))?;

    let pid = child.id();
    let started_at = crate::instance::now();

    let log_file = super::log_path(instance_id)?;
    let _ = std::fs::remove_file(&log_file);

    // Beide Ausgaben in denselben Puffer: Minecraft schreibt Fehler nach
    // stderr und den Rest nach stdout, fuer die Auswertung gehoeren sie
    // in eine Reihenfolge.
    for stream in [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn std::io::Read + Send>),
    ]
    .into_iter()
    .flatten()
    {
        let id = instance_id.to_string();
        let ev = events.clone();
        let path = log_file.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(|l| l.ok()) {
                super::write_log_line(&path, &line);
                push_line(&id, line.clone());
                ev.log_line(&id, &line);
            }
        });
    }

    let shared = std::sync::Arc::new(Mutex::new(child));
    let started_instant = std::time::Instant::now();

    registry()
        .lock()
        .map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?
        .insert(
            instance_id.to_string(),
            Entry { child: shared.clone(), pid, started_at: started_at.clone() },
        );

    // Auf das Ende warten, ohne den Aufrufer zu blockieren -- und ohne den
    // Eintrag vorher zu entfernen. Gewartet wird auf der Sperre des
    // Kindprozesses, nicht auf der der Liste; die bleibt frei.
    {
        let id = instance_id.to_string();
        std::thread::spawn(move || {
            let status = shared.lock().map(|mut c| c.wait()).ok();
            let played = started_instant.elapsed().as_secs();

            // Erst jetzt gilt die Instanz als beendet.
            if let Ok(mut reg) = registry().lock() {
                reg.remove(&id);
            }

            let code = status
                .and_then(|s| s.ok())
                .and_then(|s| s.code())
                .unwrap_or(-1);
            events.stopped(&id, code, played);
        });
    }

    Ok(RunningInstance { instance_id: instance_id.to_string(), pid, started_at })
}

/// Beendet ein laufendes Spiel.
pub fn stop(instance_id: &str) -> Result<()> {
    let reg = registry()
        .lock()
        .map_err(|_| Error::other("Zustand der laufenden Spiele ist blockiert"))?;

    let Some(entry) = reg.get(instance_id) else {
        return Err(Error::not_found("Diese Instanz läuft nicht"));
    };

    let child = entry.child.clone();
    // Die Sperre der Liste freigeben, bevor der Prozess angefasst wird.
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
    fn protokoll_wird_begrenzt() {
        let id = "test-begrenzung";
        clear_log(id);
        for i in 0..(LOG_CAPACITY + 120) {
            push_line(id, format!("Zeile {i}"));
        }
        let lines = log_of(id);
        assert_eq!(lines.len(), LOG_CAPACITY);
        // Die aeltesten sind weggefallen, die neuesten stehen noch.
        assert_eq!(lines.last().unwrap(), &format!("Zeile {}", LOG_CAPACITY + 119));
        clear_log(id);
    }

    #[test]
    fn nicht_laufende_instanz_laesst_sich_nicht_beenden() {
        assert!(stop("gibt-es-nicht").is_err());
        assert!(!is_running("gibt-es-nicht"));
    }
}
