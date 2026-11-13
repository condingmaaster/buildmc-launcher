//! Hilfen fuer das Lesen fremder JSON-Dateien.

use serde::{Deserialize, Deserializer};

/// Liest `null` als Standardwert statt als Fehler.
///
/// `#[serde(default)]` greift nur bei **fehlenden** Feldern. Steht dagegen
/// ausdruecklich `null` in der Datei, bricht das Einlesen ab. Genau das
/// schreiben andere Programme aber: Version 0.1.3 legte `"last_played": null`
/// an, und Mojangs Versions-JSON enthaelt `"classifiers": null`.
///
/// Ohne diese Behandlung scheitert das Lesen einer vollkommen gueltigen
/// Datei -- und weil Instanzen einzeln gelesen werden, faellt genau eine
/// stillschweigend aus der Liste.
pub fn null_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Probe {
        #[serde(default, deserialize_with = "super::null_default")]
        text: String,
        #[serde(default, deserialize_with = "super::null_default")]
        liste: Vec<u32>,
    }

    #[test]
    fn null_wird_zum_standardwert() {
        let p: Probe = serde_json::from_str(r#"{"text":null,"liste":null}"#).unwrap();
        assert_eq!(p, Probe { text: String::new(), liste: vec![] });
    }

    #[test]
    fn fehlende_felder_gehen_weiterhin() {
        let p: Probe = serde_json::from_str("{}").unwrap();
        assert_eq!(p.text, "");
    }

    #[test]
    fn echte_werte_bleiben_erhalten() {
        let p: Probe = serde_json::from_str(r#"{"text":"da","liste":[1,2]}"#).unwrap();
        assert_eq!(p.text, "da");
        assert_eq!(p.liste, vec![1, 2]);
    }
}
