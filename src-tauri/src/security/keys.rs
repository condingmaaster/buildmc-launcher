//! Ablage des VirusTotal-Schluessels.
//!
//! Gleiches Muster wie `ai::keys`: Umgebungsvariable hat Vorrang, sonst
//! die gemeinsame `secrets.json`. Jeder Nutzer bringt seinen eigenen,
//! persoenlichen Schluessel mit -- das ist seine private Nutzung seines
//! eigenen VirusTotal-Kontos, nicht ein vom Launcher mitgelieferter.

use crate::ai::keys::masked_of;
use crate::error::Result;

pub const ENV_VAR: &str = "VIRUSTOTAL_API_KEY";

pub fn api_key() -> Option<String> {
    if let Ok(from_env) = std::env::var(ENV_VAR) {
        let trimmed = from_env.trim().to_string();
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    crate::ai::keys::load().virustotal_api_key.filter(|k| !k.trim().is_empty())
}

pub fn is_configured() -> bool {
    api_key().is_some()
}

pub fn masked() -> Option<String> {
    api_key().as_deref().map(masked_of)
}

/// Legt den Schluessel ab. Ein leerer Wert entfernt ihn.
pub fn store(key: Option<&str>) -> Result<()> {
    crate::ai::keys::store_virustotal_key(key)
}
