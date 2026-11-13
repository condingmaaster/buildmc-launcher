//! Ablage des API-Schluessels.
//!
//! Vorrang hat eine gesetzte Umgebungsvariable -- so laesst sich der
//! Schluessel fuer einen einzelnen Start ueberschreiben, ohne die Ablage
//! anzufassen. Sonst wird die Datei neben `config.json` gelesen.
//!
//! Die Datei gehoert nie ins Projektverzeichnis und nie in ein Repo. Unter
//! Unix bekommt sie 0600, damit sie anderen Konten verschlossen bleibt.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

pub const ENV_VAR: &str = "MISTRAL_API_KEY";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Secrets {
    #[serde(default)]
    pub mistral_api_key: Option<String>,
    /// Azure-Anwendungskennung fuer die Microsoft-Anmeldung.
    ///
    /// Ein Launcher braucht eine **eigene** Registrierung, die Mojang fuer
    /// die Minecraft-Anmeldung freischaltet. Die eines anderen Launchers
    /// zu verwenden waere Missbrauch fremder Registrierung.
    #[serde(default)]
    pub ms_client_id: Option<String>,
    /// Schluessel fuer die CurseForge-Studios-API. Kostenlos ueber
    /// https://console.curseforge.com/ erhaeltlich, aber personengebunden
    /// -- auch dieser darf nicht der eines anderen Launchers sein.
    #[serde(default)]
    pub curseforge_api_key: Option<String>,
    /// Persoenlicher VirusTotal-Schluessel fuer die Sicherheitspruefung
    /// vor dem Start. Kostenlos, aber an die persoenliche, nichtkommerzielle
    /// Nutzung des eigenen Kontos gebunden.
    #[serde(default)]
    pub virustotal_api_key: Option<String>,
}

fn file() -> Result<PathBuf> {
    Ok(paths::root()?.join("secrets.json"))
}

pub fn load() -> Secrets {
    file()
        .ok()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|raw| serde_json::from_slice(&raw).ok())
        .unwrap_or_default()
}

/// Der wirksame Schluessel, oder `None` wenn keiner hinterlegt ist.
pub fn api_key() -> Option<String> {
    if let Ok(from_env) = std::env::var(ENV_VAR) {
        let trimmed = from_env.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    load().mistral_api_key.filter(|k| !k.trim().is_empty())
}

pub fn is_configured() -> bool {
    api_key().is_some()
}

pub const CURSEFORGE_ENV_VAR: &str = "CURSEFORGE_API_KEY";

/// Der CurseForge-Schluessel, oder `None` wenn keiner hinterlegt ist.
pub fn curseforge_api_key() -> Option<String> {
    if let Ok(from_env) = std::env::var(CURSEFORGE_ENV_VAR) {
        let trimmed = from_env.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    load().curseforge_api_key.filter(|k| !k.trim().is_empty())
}

/// Legt den CurseForge-Schluessel ab. Ein leerer Wert entfernt ihn.
pub fn store_curseforge_key(key: Option<&str>) -> Result<()> {
    let path = file()?;
    paths::ensure_parent(&path)?;
    let mut secrets = load();
    secrets.curseforge_api_key = key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string);
    std::fs::write(&path, serde_json::to_vec_pretty(&secrets)?)?;
    restrict(&path)?;
    Ok(())
}

pub fn curseforge_masked() -> Option<String> {
    curseforge_api_key().as_deref().map(masked_of)
}

pub fn store_virustotal_key(key: Option<&str>) -> Result<()> {
    let path = file()?;
    paths::ensure_parent(&path)?;
    let mut secrets = load();
    secrets.virustotal_api_key = key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string);
    std::fs::write(&path, serde_json::to_vec_pretty(&secrets)?)?;
    restrict(&path)?;
    Ok(())
}

pub const MS_ENV_VAR: &str = "BUILDMC_MS_CLIENT_ID";

/// Die Anwendungskennung fuer die Microsoft-Anmeldung.
pub fn ms_client_id() -> Option<String> {
    if let Ok(from_env) = std::env::var(MS_ENV_VAR) {
        let trimmed = from_env.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    load().ms_client_id.filter(|k| !k.trim().is_empty())
}

/// Legt die Anwendungskennung ab. Ein leerer Wert entfernt sie.
pub fn store_ms_client_id(id: Option<&str>) -> Result<()> {
    let path = file()?;
    paths::ensure_parent(&path)?;
    let mut secrets = load();
    secrets.ms_client_id = id
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string);
    std::fs::write(&path, serde_json::to_vec_pretty(&secrets)?)?;
    restrict(&path)?;
    Ok(())
}

/// Legt den Schluessel ab. Ein leerer Wert entfernt ihn.
pub fn store(key: Option<&str>) -> Result<()> {
    let path = file()?;
    paths::ensure_parent(&path)?;

    let mut secrets = load();
    secrets.mistral_api_key = key
        .map(str::trim)
        .filter(|k| !k.is_empty())
        .map(str::to_string);

    std::fs::write(&path, serde_json::to_vec_pretty(&secrets)?)?;
    restrict(&path)?;
    Ok(())
}

#[cfg(unix)]
fn restrict(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

/// Getrennt testbarer Kern des Abblendens.
pub fn masked_of(key: &str) -> String {
    let tail: String = key.chars().rev().take(4).collect::<Vec<_>>()
        .into_iter().rev().collect();
    format!("\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}{tail}")
}

/// Der hinterlegte Schluessel, abgeblendet. Der volle Wert verlaesst das
/// Backend nie -- die Oberflaeche bekommt nur diese Fassung zu sehen.
pub fn masked() -> Option<String> {
    api_key().as_deref().map(masked_of)
}

#[cfg(test)]
mod tests {
    use super::masked_of;

    #[test]
    fn abblenden_zeigt_nur_das_ende() {
        assert_eq!(masked_of("abcdefghijkl"), "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}ijkl");
        assert_eq!(masked_of("ab"), "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}ab");
    }
}
