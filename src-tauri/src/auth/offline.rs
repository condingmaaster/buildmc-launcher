//! Konten ohne Microsoft-Anmeldung.
//!
//! Die UUID wird wie in Minecrafts Offline-Modus aus `OfflinePlayer:<name>`
//! als UUID Version 3 gebildet. Dadurch bleibt sie zu demselben Namen
//! stabil -- Weltdaten, Inventare und Fortschritte gehen zwischen Starts
//! nicht verloren.

use crate::error::{Error, Result};

/// Erzeugt die Offline-UUID zu einem Namen.
pub fn offline_uuid(username: &str) -> uuid::Uuid {
    uuid::Uuid::new_v3(
        &uuid::Uuid::NAMESPACE_OID,
        format!("OfflinePlayer:{username}").as_bytes(),
    )
}

/// Minecraft laesst 3 bis 16 Zeichen aus Buchstaben, Ziffern und `_` zu.
pub fn validate_username(name: &str) -> Result<()> {
    let n = name.trim();
    if n.len() < 3 || n.len() > 16 {
        return Err(Error::other("Der Name muss 3 bis 16 Zeichen lang sein"));
    }
    if !n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(Error::other(
            "Erlaubt sind nur Buchstaben, Ziffern und Unterstriche",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_ist_zum_namen_stabil() {
        assert_eq!(offline_uuid("Jona"), offline_uuid("Jona"));
        assert_ne!(offline_uuid("Jona"), offline_uuid("jona"));
    }

    #[test]
    fn namen_werden_geprueft() {
        assert!(validate_username("Jona").is_ok());
        assert!(validate_username("Coding_Master1").is_ok());
        assert!(validate_username("ab").is_err());
        assert!(validate_username("viel_zu_langer_name").is_err());
        assert!(validate_username("mit leer").is_err());
        assert!(validate_username("umlautä").is_err());
    }
}
