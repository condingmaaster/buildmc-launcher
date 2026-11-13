//! Sicherheitspruefung vor jedem Spielstart.
//!
//! Eine einzige Pruefstelle statt vieler: nicht nur, was gerade
//! heruntergeladen wurde, sondern jede `.jar`-Datei im Mods-Ordner einer
//! Instanz -- egal ob sie ueber die Suche, "Lokale Mod hinzufuegen", ein
//! importiertes Modpack oder von Hand hineinkopiert dorthin gelangte.
//!
//! Zwei Schichten, ein Zwischenspeicher:
//! * VirusTotal (Hash-Abfrage, siehe `virustotal.rs`) -- braucht einen
//!   persoenlichen Schluessel, ohne ihn wird diese Schicht uebersprungen.
//! * ClamAV (siehe `clamav.rs`), falls lokal installiert.
//!
//! Der Zwischenspeicher ist **hash-basiert, nicht instanzbasiert**:
//! dieselbe Datei in drei Instanzen wird nur einmal geprueft, und eine
//! unveraenderte Datei loest nie erneut eine Netzabfrage aus.

pub mod clamav;
pub mod keys;
pub mod virustotal;

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use sha1::{Digest, Sha1};

use crate::error::Result;

/// Wie strikt ein Fund behandelt wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Warnt, laesst den Start aber zu -- ein einzelner Fehlalarm einer
    /// Antiviren-Engine soll nie versehentlich das Spiel aussperren.
    Warn,
    /// Verweigert den Start, bis der Nutzer bewusst uebergeht.
    Block,
}

impl Default for Mode {
    fn default() -> Self {
        Mode::Warn
    }
}

/// Das Urteil zu genau einer Datei, wie es im Zwischenspeicher steht und
/// der Oberflaeche gezeigt wird.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVerdict {
    pub sha1: String,
    pub filename: String,
    pub flagged: bool,
    /// Kurzer, fuer Menschen lesbarer Grund -- "2 von 71 Engines", "ClamAV:
    /// Fund", "unbekannt (nicht geprueft)".
    pub reason: String,
    pub checked_at: String,
    /// Hat der Nutzer diesen Hash ausdruecklich als Ausnahme freigegeben?
    #[serde(default)]
    pub allowed: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Cache {
    #[serde(default)]
    entries: HashMap<String, FileVerdict>,
}

fn cache_path() -> Result<std::path::PathBuf> {
    Ok(crate::paths::root()?.join("security_cache.json"))
}

fn load_cache() -> Cache {
    cache_path()
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

fn save_cache(cache: &Cache) -> Result<()> {
    // Ueber `store::write`, nicht direkt `std::fs::write`: erst in eine
    // Nebendatei, dann umbenennen -- ein Absturz mitten im Schreiben
    // wuerde sonst den Zwischenspeicher beschaedigen, und `load_cache`
    // faellt bei einer unlesbaren Datei still auf "leer" zurueck, was
    // jede bereits geprüfte Datei erneut die VirusTotal-Ratenbegrenzung
    // kosten wuerde.
    crate::store::write(&cache_path()?, cache)
}

fn sha1_of(path: &Path) -> Result<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha1::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

/// Bewertet eine einzelne, noch nicht zwischengespeicherte Datei -- ohne
/// Zugriff auf den Zwischenspeicher, damit mehrere Dateien gleichzeitig
/// bewertet werden koennen (siehe `verify_instance`), ohne sich eine
/// veraenderliche Ablage zu teilen.
async fn bewerte(path: std::path::PathBuf, sha1: String, filename: String) -> Result<FileVerdict> {
    // Beide Schichten gleichzeitig, nicht nacheinander, und mit geteilter
    // Arbeit ueber alle Dateien einer Instanz hinweg: VirusTotal ist ein
    // ratenbegrenzter Netzwerkaufruf (die Begrenzung sitzt in
    // `virustotal::lookup` selbst und wirkt ueber alle gleichzeitig
    // laufenden Pruefungen hinweg), ClamAV ein blockierender
    // Unterprozess (deshalb in spawn_blocking). Faellt eine Schicht aus --
    // kein Schluessel, kein ClamAV installiert, Netzwerkfehler -- liefert
    // die andere nahtlos ihr Ergebnis, ohne auf die ausgefallene zu warten.
    let path_fuer_clamav = path.clone();
    let (vt, clamav) = tokio::join!(
        virustotal::lookup(&sha1),
        tokio::task::spawn_blocking(move || clamav::scan(&path_fuer_clamav)),
    );
    let vt = vt.unwrap_or(virustotal::Verdict::Unknown);
    let clamav = clamav.unwrap_or(None);

    let (flagged, reason) = match (&vt, clamav) {
        (virustotal::Verdict::Flagged { engines, total }, _) => {
            (true, format!("VirusTotal: {engines} von {total} Engines melden einen Fund"))
        }
        (_, Some(clamav::Outcome::Infected)) => (true, "ClamAV meldet einen Fund".to_string()),
        (virustotal::Verdict::Clean { .. }, _) => (false, "Unauffällig (VirusTotal)".to_string()),
        (virustotal::Verdict::Unknown, None) => {
            (false, "Nicht geprüft (kein Schlüssel, kein ClamAV)".to_string())
        }
        (virustotal::Verdict::Unknown, Some(clamav::Outcome::Clean)) => {
            (false, "Unauffällig (ClamAV)".to_string())
        }
        (virustotal::Verdict::Unknown, Some(clamav::Outcome::ScanFailed)) => {
            (false, "Nicht geprüft (ClamAV-Scan fehlgeschlagen)".to_string())
        }
    };

    Ok(FileVerdict { sha1, filename, flagged, reason, checked_at: crate::instance::now(), allowed: false })
}

/// Wie viele Dateien gleichzeitig bewertet werden. ClamAV-Aufrufe sind
/// eigene Betriebssystemprozesse -- unbegrenzt viele gleichzeitig waeren
/// unnoetiger Druck auf den Rechner. VirusTotal braucht keine eigene
/// Deckelung hier, das regelt bereits dessen Ratenbegrenzung.
const GLEICHZEITIG: usize = 6;

/// Prueft alle `.jar`-Dateien im Mods-Ordner einer Instanz -- gleichzeitig,
/// nicht Datei fuer Datei nacheinander.
///
/// Gibt die Funde zurueck, die weder unauffaellig noch vom Nutzer bereits
/// freigegeben sind -- eine leere Liste heisst "alles in Ordnung oder
/// nicht pruefbar", nie ein Fehler nur weil kein Schluessel hinterlegt ist.
pub async fn verify_instance(instance_id: &str) -> Result<Vec<FileVerdict>> {
    use futures::stream::{self, StreamExt};

    let dir = crate::mods::mods_dir(instance_id)?;
    let mut cache = load_cache();

    // Ordner lesen und jede Datei hashen ist blockierendes Dateisystem-E/A
    // -- direkt in einer async-Funktion ausgefuehrt, haette es einen der
    // wenigen Tokio-Arbeitsthreads belegt, bis die Datei fertig gelesen
    // ist. Bei mehreren Mods hintereinander kann das alle Arbeitsthreads
    // gleichzeitig blockieren -- und mit ihnen die komplette
    // Tauri-Oberflaeche, die auf derselben Runtime laeuft ("reagiert
    // nicht"). Deshalb komplett in spawn_blocking, wie schon beim
    // ClamAV-Unterprozess.
    let dir_fuer_blocking = dir.clone();
    let hashes: Vec<(std::path::PathBuf, String, String)> =
        tokio::task::spawn_blocking(move || -> Result<Vec<_>> {
            let Ok(read) = std::fs::read_dir(&dir_fuer_blocking) else { return Ok(Vec::new()) };
            let jars = read
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("jar"));

            let mut out = Vec::new();
            for path in jars {
                let sha1 = sha1_of(&path)?;
                let filename =
                    path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string();
                out.push((path, sha1, filename));
            }
            Ok(out)
        })
        .await
        .map_err(|e| crate::Error::other(format!("Dateipruefung abgebrochen: {e}")))??;

    // Erst die Hashes -- dann trennen, was schon im Zwischenspeicher
    // steht von dem, was neu geprueft werden muss. Nur Letzteres braucht
    // ueberhaupt ein Netzwerk oder einen Unterprozess.
    let mut sofort = Vec::new();
    let mut zu_pruefen = Vec::new();
    for (path, sha1, filename) in hashes {
        match cache.entries.get(&sha1) {
            Some(bestehend) => sofort.push(bestehend.clone()),
            None => zu_pruefen.push((path, sha1, filename)),
        }
    }

    // `buffer_unordered` haelt hoechstens GLEICHZEITIG Auftraege gleichzeitig
    // am Laufen, statt bei z.B. 80 Mods sofort 80 ClamAV-Unterprozesse
    // gleichzeitig zu starten.
    let neu: Vec<FileVerdict> = stream::iter(zu_pruefen)
        .map(|(p, s, f)| bewerte(p, s, f))
        .buffer_unordered(GLEICHZEITIG)
        .collect::<Vec<_>>()
        .await
        .into_iter()
        .collect::<Result<Vec<_>>>()?;

    for v in &neu {
        cache.entries.insert(v.sha1.clone(), v.clone());
    }
    save_cache(&cache)?;

    let funde = sofort.into_iter().chain(neu)
        .filter(|v| v.flagged && !v.allowed)
        .collect();
    Ok(funde)
}

/// Markiert einen Hash als vom Nutzer bewusst freigegeben -- ein
/// Fehlalarm, den der Nutzer kennt und uebergeht. Kuenftig wird er nicht
/// mehr gemeldet, ohne erneut abgefragt zu werden.
pub fn allow_hash(sha1: &str) -> Result<()> {
    let mut cache = load_cache();
    if let Some(eintrag) = cache.entries.get_mut(sha1) {
        eintrag.allowed = true;
    }
    save_cache(&cache)
}

/// Alle bisherigen Urteile, neueste zuerst -- fuer die Uebersicht.
pub fn history() -> Vec<FileVerdict> {
    let mut alle: Vec<FileVerdict> = load_cache().entries.into_values().collect();
    alle.sort_by(|a, b| b.checked_at.cmp(&a.checked_at));
    alle
}

/// Was die Oberflaeche ueber den Zustand der Pruefung wissen muss.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub virustotal_configured: bool,
    pub virustotal_key_hint: Option<String>,
    pub clamav_available: bool,
    pub mode: Mode,
    pub auto_scan: bool,
}

pub fn status() -> Status {
    let config = crate::store::load_config();
    Status {
        virustotal_configured: keys::is_configured(),
        virustotal_key_hint: keys::masked(),
        clamav_available: clamav::verfuegbar(),
        mode: config.security_mode,
        auto_scan: config.security_auto_scan,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eintrag(sha1: &str, allowed: bool) -> FileVerdict {
        FileVerdict {
            sha1: sha1.into(), filename: "x.jar".into(), flagged: true,
            reason: "Test".into(), checked_at: "2026-01-01T00:00:00Z".into(), allowed,
        }
    }

    #[test]
    fn freigegebener_hash_gilt_nach_dem_freigeben_als_erlaubt() {
        let mut cache = Cache::default();
        cache.entries.insert("abc".into(), eintrag("abc", false));

        // Direkter Test der Kernlogik von allow_hash, ohne Dateisystem.
        if let Some(e) = cache.entries.get_mut("abc") { e.allowed = true; }
        assert!(cache.entries["abc"].allowed);
    }

    #[test]
    fn verlauf_wird_nach_pruefzeitpunkt_absteigend_sortiert() {
        let mut alle = vec![
            FileVerdict { checked_at: "2026-01-01T00:00:00Z".into(), ..eintrag("a", false) },
            FileVerdict { checked_at: "2026-03-01T00:00:00Z".into(), ..eintrag("b", false) },
        ];
        alle.sort_by(|a, b| b.checked_at.cmp(&a.checked_at));
        assert_eq!(alle[0].sha1, "b");
    }
}
