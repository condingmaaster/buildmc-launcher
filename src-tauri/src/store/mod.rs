//! Ablage auf der Platte.
//!
//! Die Dateinamen und Feldnamen sind unveraendert aus Version 0.1.3
//! uebernommen. Eine bestehende Installation unter
//! `~/.local/share/buildmc-launcher/` laeuft damit ohne Migration weiter --
//! das war die Bedingung, unter der der Neubau ueberhaupt Sinn ergibt.

pub mod config;

use std::path::Path;

use serde::{de::DeserializeOwned, Serialize};

use crate::error::Result;
use crate::paths;

pub use config::Config;

/// Liest eine JSON-Datei. Fehlt sie oder ist sie unlesbar, kommt der
/// Standardwert -- eine beschaedigte Einstellungsdatei darf den Start
/// nicht verhindern.
pub fn read_or_default<T: DeserializeOwned + Default>(path: &Path) -> T {
    std::fs::read(path)
        .ok()
        .and_then(|raw| match serde_json::from_slice(&raw) {
            Ok(value) => Some(value),
            Err(e) => {
                eprintln!("[store] {} ist unlesbar: {e}", path.display());
                None
            }
        })
        .unwrap_or_default()
}

/// Liest eine JSON-Datei, die vorhanden sein muss.
pub fn read<T: DeserializeOwned>(path: &Path) -> Result<T> {
    let raw = std::fs::read(path)?;
    Ok(serde_json::from_slice(&raw)?)
}

/// Schreibt eine JSON-Datei.
///
/// Erst in eine Nebendatei, dann umbenennen: ein Absturz mitten im
/// Schreiben darf die vorhandene Datei nicht zerstoeren. Bei Instanzen
/// und Konten waere das der Verlust von Spielstaenden bzw. Anmeldungen.
pub fn write<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    paths::ensure_parent(path)?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(value)?)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/* ---- Konten und Server ------------------------------------------------ */

pub fn load_config() -> Config {
    paths::config_file()
        .map(|p| read_or_default::<Config>(&p))
        .unwrap_or_default()
}

pub fn save_config(config: &Config) -> Result<()> {
    write(&paths::config_file()?, config)
}

pub fn load_accounts() -> Vec<crate::auth::Account> {
    paths::accounts_file()
        .map(|p| read_or_default::<Vec<_>>(&p))
        .unwrap_or_default()
}

pub fn save_accounts(accounts: &[crate::auth::Account]) -> Result<()> {
    write(&paths::accounts_file()?, &accounts)
}

/// Schuetzt "laden, aendern, speichern" der Kontenliste als eine Einheit.
///
/// Ohne das koennten zwei fast gleichzeitige Aenderungen -- zwei schnelle
/// Klicks, oder ein Spielstart, der nebenbei ein Token erneuert, waehrend
/// die Oberflaeche ein Konto entfernt -- sich gegenseitig ueberschreiben:
/// wer zuletzt speichert, gewinnt, und die andere Aenderung ist spurlos
/// weg. Der Schalter wird nur waehrend des synchronen Lesens/Schreibens
/// gehalten, nie ueber ein `.await` hinweg -- ein Netzwerkaufruf (etwa eine
/// Token-Erneuerung) muss vorher abgeschlossen sein.
static ACCOUNTS_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn with_accounts<T>(
    f: impl FnOnce(&mut Vec<crate::auth::Account>) -> Result<T>,
) -> Result<T> {
    // Ein vergifteter Schalter (nach einem Absturz mitten in f) darf
    // spaetere Aenderungen nicht fuer immer blockieren.
    let _guard = ACCOUNTS_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Hier nicht load_accounts(): das macht aus einer unlesbaren Datei eine
    // leere Liste, und das Speichern danach loeschte still alle Konten.
    let path = paths::accounts_file()?;
    let mut accounts: Vec<crate::auth::Account> = if path.exists() {
        read(&path).map_err(|e| {
            crate::Error::other(format!("{} ist unlesbar ({e}); die Konten bleiben unverändert", path.display()))
        })?
    } else {
        Vec::new()
    };
    let result = f(&mut accounts)?;
    save_accounts(&accounts)?;
    Ok(result)
}

pub fn load_servers() -> Vec<crate::server::SavedServer> {
    paths::servers_file()
        .map(|p| read_or_default::<Vec<_>>(&p))
        .unwrap_or_default()
}

pub fn save_servers(servers: &[crate::server::SavedServer]) -> Result<()> {
    write(&paths::servers_file()?, &servers)
}
