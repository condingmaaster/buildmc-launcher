//! ClamAV als externer Prozess -- nie als eingebundene Bibliothek.
//!
//! ClamAV steht unter GPL v2. Das betrifft nur das *Linken* einer
//! Bibliothek in eigenen Code, nicht das *Aufrufen* eines eigenstaendigen
//! Programms -- deshalb wird hier ausschliesslich ein ClamAV-Programm als
//! Prozess gestartet, nie eine ClamAV-Bibliothek eingebunden. Damit
//! bleibt der eigene Code proprietaer, ClamAV bleibt fuer sich GPL.
//!
//! Zwei Wege, `clamdscan` bevorzugt -- mit einer Einschraenkung, die sich
//! erst am echten System zeigte: `clamd` laeuft als eigener Systembenutzer
//! (`clamav`) und kommt deshalb an keine Datei in einem privaten
//! Home-Verzeichnis heran (typischerweise `700`), unabhaengig von den
//! Rechten der Datei selbst -- der Zugriff scheitert schon beim
//! Durchqueren des Ordners. Genau dort liegen die Mod-Ordner dieses
//! Launchers. `clamdscan` meldet dafuer keinen eigenen Exitcode, sondern
//! denselben "Scan fehlgeschlagen" wie bei jedem anderen Problem --
//! deshalb wird bei einem Fehlschlag automatisch mit `clamscan`
//! nachgelegt, das als der aufrufende Nutzer selbst laeuft und deshalb
//! ganz gewoehnlich Zugriff hat.
//!
//! * **`clamdscan`** spricht mit dem laufenden `clamd`-Dienst, dessen
//!   Signaturdatenbank schon im Speicher liegt -- ein einzelner Scan
//!   dauert dadurch Millisekunden statt der ein bis zwei Sekunden, die
//!   `clamscan` fuer das Laden der Datenbank bei jedem Aufruf braucht.
//!   Erster Versuch, wenn verfuegbar.
//! * **`clamscan`** laeuft eigenstaendig unter den Rechten des Aufrufers
//!   -- kein Berechtigungsproblem mit privaten Ordnern, dafuer langsamer.
//!   Rueckfall, wenn `clamdscan` fehlt oder an genau dieser Datei scheitert.
//!
//! Exitcodes bei beiden identisch (siehe `man clamscan`/`man clamdscan`):
//! `0` sauber, `1` Fund, alles andere ein Fehler beim Scannen selbst --
//! kein Urteil ueber die Datei.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    Clean,
    Infected,
    /// Scan selbst ist fehlgeschlagen (fehlende Datenbank, kein Zugriff,
    /// …) -- sagt nichts ueber die Datei aus.
    ScanFailed,
}

fn deute_exitcode(code: Option<i32>) -> Outcome {
    match code {
        Some(0) => Outcome::Clean,
        Some(1) => Outcome::Infected,
        _ => Outcome::ScanFailed,
    }
}

fn programm_da(name: &str) -> bool {
    Command::new(name).arg("--version").output().map(|o| o.status.success()).unwrap_or(false)
}

/// Ist mindestens eines der beiden Programme auf diesem Rechner installiert?
pub fn verfuegbar() -> bool {
    programm_da("clamdscan") || programm_da("clamscan")
}

fn scan_mit(bin: &str, path: &Path) -> Option<Outcome> {
    let mut cmd = Command::new(bin);
    cmd.arg("--no-summary");
    // "--infected" (nur auffaellige Zeilen ausgeben) kennt nur clamscan --
    // clamdscan lehnt unbekannte Optionen ab.
    if bin == "clamscan" {
        cmd.arg("--infected");
    }
    let output = cmd.arg(path).output().ok()?;
    Some(deute_exitcode(output.status.code()))
}

/// Scannt eine einzelne Datei. `None`, wenn weder `clamdscan` noch
/// `clamscan` installiert sind -- dann wird diese Schicht schlicht
/// uebersprungen, kein Fehler. Blockierend: vom Aufrufer ueber
/// `tokio::task::spawn_blocking` einzubetten, damit sie parallel zur
/// VirusTotal-Abfrage laufen kann statt sie zu blockieren.
///
/// Erst `clamdscan` (schnell, aber ohne Zugriff auf private Ordner),
/// bei einem Fehlschlag sofort `clamscan` (langsamer, aber mit den
/// Rechten des Aufrufers) -- statt bei einem blossen
/// Berechtigungsproblem des Dienstes die ganze Schicht aufzugeben.
pub fn scan(path: &Path) -> Option<Outcome> {
    if programm_da("clamdscan") {
        if let Some(ergebnis) = scan_mit("clamdscan", path) {
            if ergebnis != Outcome::ScanFailed {
                return Some(ergebnis);
            }
            // clamdscan konnte die Datei nicht lesen (oder ein anderes
            // Problem) -- clamscan hat eigene Rechte, vielleicht klappt es.
        }
    }
    if programm_da("clamscan") {
        return scan_mit("clamscan", path);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exitcode_null_ist_sauber() {
        assert_eq!(deute_exitcode(Some(0)), Outcome::Clean);
    }

    #[test]
    fn exitcode_eins_ist_ein_fund() {
        assert_eq!(deute_exitcode(Some(1)), Outcome::Infected);
    }

    #[test]
    fn andere_exitcodes_sind_ein_scanfehler_kein_urteil() {
        // 2 (clamdscan) und die feineren clamscan-Fehlercodes gleichermassen.
        assert_eq!(deute_exitcode(Some(2)), Outcome::ScanFailed);
        assert_eq!(deute_exitcode(Some(40)), Outcome::ScanFailed);
        assert_eq!(deute_exitcode(Some(52)), Outcome::ScanFailed);
        assert_eq!(deute_exitcode(None), Outcome::ScanFailed);
    }
}
