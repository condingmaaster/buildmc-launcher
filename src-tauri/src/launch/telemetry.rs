//! Live-Werte eines laufenden Spiels.
//!
//! Gemessen wird von aussen, ueber den Prozess. Das begrenzt, was moeglich
//! ist: Arbeitsspeicher des Prozesses, Prozessorlast und Laufzeit lassen
//! sich lesen -- **die Java-Halde und die Pausen des Garbage Collectors
//! nicht**. Dafuer braeuchte es eine Anbindung im Spiel selbst.
//!
//! Deshalb wird hier nur gemeldet, was wirklich gemessen ist. Als Bezug
//! dient der Hoechstwert, den der Launcher der Instanz per `-Xmx` gegeben
//! hat -- der ist bekannt und ergibt eine ehrliche Auslastungsanzeige.

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct Telemetry {
    pub instance_id: String,
    /// Tatsaechlich belegter Arbeitsspeicher des Prozesses.
    pub rss_bytes: u64,
    /// Der Instanz zugestandene Obergrenze, aus `-Xmx`.
    pub max_bytes: u64,
    pub cpu_percent: f32,
    pub uptime_seconds: u64,
}

/// Liest die Werte einer laufenden Instanz.
///
/// Gibt `None` zurueck, wenn die Instanz nicht laeuft oder der Prozess
/// inzwischen verschwunden ist.
pub fn sample(instance_id: &str) -> Option<Telemetry> {
    use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};

    let running = super::running();
    let entry = running.iter().find(|r| r.instance_id == instance_id)?;

    let mut sys = System::new_with_specifics(
        RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
    );
    let pid = Pid::from_u32(entry.pid);
    sys.refresh_processes(sysinfo::ProcessesToUpdate::Some(&[pid]), true);

    let process = sys.process(pid)?;

    let started = chrono::DateTime::parse_from_rfc3339(&entry.started_at)
        .map(|d| d.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());

    // Die Obergrenze kennt der Launcher: er hat sie selbst gesetzt.
    let max_bytes = crate::instance::load(instance_id)
        .map(|i| i.max_ram as u64 * 1024 * 1024)
        .unwrap_or(0);

    Some(Telemetry {
        instance_id: instance_id.to_string(),
        rss_bytes: process.memory(),
        max_bytes,
        cpu_percent: process.cpu_usage(),
        uptime_seconds: (chrono::Utc::now().timestamp() - started).max(0) as u64,
    })
}

/// Werte aller laufenden Instanzen.
pub fn sample_all() -> Vec<Telemetry> {
    super::running()
        .into_iter()
        .filter_map(|r| sample(&r.instance_id))
        .collect()
}
