//! Was ein Modell annimmt -- zur Laufzeit gelernt.
//!
//! Mistrals Modelle unterscheiden sich darin, welche Zusatzparameter sie
//! annehmen. Mistral Medium 3.5 fuehrt in seiner Fähigkeitsliste
//! „Structured Outputs" und „Function Calling", aber **kein** Reasoning --
//! `reasoning_effort` ergibt dort eine 422.
//!
//! Eine fest eingetragene Tabelle waere die naheliegende Antwort und die
//! falsche: sie veraltet mit dem naechsten Modellwechsel, und bis dahin
//! sieht ein voellig gueltiges Modell wie ein gesperrtes aus.
//!
//! Stattdessen wird probiert. Lehnt ein Modell einen Zusatzparameter ab,
//! wird er beim naechsten Versuch weggelassen und das gemerkt -- ab dann
//! kostet es keinen Fehlversuch mehr.

use std::collections::HashMap;
use std::sync::Mutex;

/// Ein Zusatzparameter, der abgelehnt werden kann.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Extra {
    /// `reasoning_effort`
    Reasoning,
    /// `response_format: json_object`
    JsonFormat,
}

impl Extra {
    pub fn key(self) -> &'static str {
        match self {
            Extra::Reasoning => "reasoning_effort",
            Extra::JsonFormat => "response_format",
        }
    }
}

fn rejected() -> &'static Mutex<HashMap<(String, Extra), bool>> {
    static R: std::sync::OnceLock<Mutex<HashMap<(String, Extra), bool>>> =
        std::sync::OnceLock::new();
    R.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Darf dieser Parameter bei diesem Modell mitgeschickt werden?
pub fn allowed(model: &str, extra: Extra) -> bool {
    rejected()
        .lock()
        .map(|m| !m.get(&(model.to_string(), extra)).copied().unwrap_or(false))
        .unwrap_or(true)
}

/// Merkt sich, dass ein Modell diesen Parameter nicht annimmt.
pub fn mark_rejected(model: &str, extra: Extra) {
    if let Ok(mut m) = rejected().lock() {
        m.insert((model.to_string(), extra), true);
        eprintln!("[ai] {model} nimmt {} nicht an — wird künftig weggelassen", extra.key());
    }
}

/// Entfernt die Zusatzparameter aus einem Anfragekoerper.
///
/// Gibt zurueck, welche tatsaechlich entfernt wurden -- war keiner dabei,
/// hilft ein erneuter Versuch nicht und der Fehler liegt woanders.
pub fn strip_extras(body: &mut serde_json::Value) -> Vec<Extra> {
    let mut removed = Vec::new();
    let Some(map) = body.as_object_mut() else { return removed };

    for extra in [Extra::Reasoning, Extra::JsonFormat] {
        if map.remove(extra.key()).is_some() {
            removed.push(extra);
        }
    }
    removed
}

/// Setzt die Zusatzparameter, sofern das Modell sie bisher angenommen hat.
pub fn apply_extras(body: &mut serde_json::Value, model: &str, want_reasoning: bool, want_json: bool) {
    let Some(map) = body.as_object_mut() else { return };

    if want_json && allowed(model, Extra::JsonFormat) {
        map.insert(
            Extra::JsonFormat.key().into(),
            serde_json::json!({ "type": "json_object" }),
        );
    }
    if want_reasoning && allowed(model, Extra::Reasoning) {
        map.insert(Extra::Reasoning.key().into(), serde_json::json!("high"));
    }
}

/// Was ein Modell abgelehnt hat -- fuer die Anzeige in den Einstellungen.
///
/// Ohne diese Auskunft wundert sich der Nutzer, warum eine eingeschaltete
/// Einstellung wirkungslos bleibt.
pub fn rejections(model: &str) -> Vec<&'static str> {
    let Ok(map) = rejected().lock() else { return Vec::new() };
    [Extra::Reasoning, Extra::JsonFormat]
        .into_iter()
        .filter(|e| map.get(&(model.to_string(), *e)).copied().unwrap_or(false))
        .map(|e| e.key())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zusatzparameter_werden_entfernt_und_gemeldet() {
        let mut body = serde_json::json!({
            "model": "x",
            "reasoning_effort": "high",
            "response_format": { "type": "json_object" },
            "messages": [],
        });
        let removed = strip_extras(&mut body);
        assert_eq!(removed.len(), 2);
        assert!(body.get("reasoning_effort").is_none());
        assert!(body.get("response_format").is_none());
        // Der Rest bleibt unangetastet.
        assert_eq!(body["model"], "x");
    }

    #[test]
    fn ohne_zusatzparameter_gibt_es_nichts_zu_entfernen() {
        // Dann hilft ein erneuter Versuch nicht -- der Fehler liegt woanders.
        let mut body = serde_json::json!({ "model": "x", "messages": [] });
        assert!(strip_extras(&mut body).is_empty());
    }

    #[test]
    fn abgelehntes_wird_kuenftig_weggelassen() {
        let modell = "test-modell-a";
        assert!(allowed(modell, Extra::Reasoning));

        mark_rejected(modell, Extra::Reasoning);
        assert!(!allowed(modell, Extra::Reasoning));
        // Nur dieser eine Parameter, nicht alle.
        assert!(allowed(modell, Extra::JsonFormat));
        // Und nur dieses Modell.
        assert!(allowed("test-modell-b", Extra::Reasoning));
    }

    #[test]
    fn gemerkte_ablehnung_wirkt_beim_setzen() {
        let modell = "test-modell-c";
        mark_rejected(modell, Extra::JsonFormat);

        let mut body = serde_json::json!({ "model": modell });
        apply_extras(&mut body, modell, true, true);

        assert!(body.get("response_format").is_none(), "abgelehnt, also weglassen");
        assert_eq!(body["reasoning_effort"], "high", "das andere bleibt");
    }
}
