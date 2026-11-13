//! Ablaufzeitpunkt eines Tokens lesen.
//!
//! Launcher schreiben dieses Feld unterschiedlich: Version 0.1.3 legte es
//! als RFC-3339-Zeichenkette ab (`2026-08-13T19:27:34.391Z`), andere
//! schreiben Unix-Sekunden oder -Millisekunden. Wird das falsch gelesen,
//! gilt ein gueltiges Token als abgelaufen -- oder schlimmer, ein
//! abgelaufenes als gueltig.
//!
//! Die Erkennung arbeitet deshalb von allein und stuetzt sich auf die
//! Groessenordnung: Sekunden seit 1970 liegen heute bei etwa 1,8e9,
//! Millisekunden bei 1,8e12. Dazwischen liegen drei Zehnerpotenzen --
//! eine Verwechslung ist praktisch ausgeschlossen.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Wie das Feld gelesen wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpiryFormat {
    /// Erkennt das Format am Wert. Die richtige Wahl, solange nichts
    /// dagegen spricht.
    #[default]
    Auto,
    /// Zeichenkette nach RFC 3339, wie in 0.1.3.
    Rfc3339,
    /// Ganzzahl, Sekunden seit 1970.
    UnixSeconds,
    /// Ganzzahl, Millisekunden seit 1970.
    UnixMillis,
}

/// Ab hier ist ein Wert zu gross fuer Sekunden und muss Millisekunden sein.
/// Entspricht dem Jahr 5138 in Sekunden -- ein Zeitpunkt, den kein Token
/// je erreicht.
const MILLIS_THRESHOLD: i64 = 100_000_000_000;

/// Liest einen Wert beliebigen Formats als Unix-Sekunden.
pub fn parse(value: &serde_json::Value, format: ExpiryFormat) -> i64 {
    match value {
        serde_json::Value::Number(n) => {
            let raw = n.as_i64().unwrap_or(0);
            match format {
                ExpiryFormat::UnixMillis => raw / 1000,
                ExpiryFormat::UnixSeconds => raw,
                // Auto und Rfc3339: an der Groessenordnung entscheiden.
                _ if raw >= MILLIS_THRESHOLD => raw / 1000,
                _ => raw,
            }
        }
        serde_json::Value::String(s) => parse_str(s, format),
        _ => 0,
    }
}

fn parse_str(s: &str, format: ExpiryFormat) -> i64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return 0;
    }

    // Eine Zahl in Anfuehrungszeichen kommt vor -- dann wie eine Zahl lesen.
    if format != ExpiryFormat::Rfc3339 {
        if let Ok(n) = trimmed.parse::<i64>() {
            return match format {
                ExpiryFormat::UnixMillis => n / 1000,
                ExpiryFormat::UnixSeconds => n,
                _ if n >= MILLIS_THRESHOLD => n / 1000,
                _ => n,
            };
        }
    }

    chrono::DateTime::parse_from_rfc3339(trimmed)
        .map(|d| d.timestamp())
        .unwrap_or(0)
}

/// Serde-Helfer: liest das Feld in jedem Format.
///
/// Beim Schreiben werden immer Unix-Sekunden abgelegt -- eindeutig und
/// ohne Zeitzonenfallen. Gelesen wird trotzdem weiterhin alles.
pub fn deserialize<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(parse(&value, ExpiryFormat::Auto))
}

pub fn serialize<S: Serializer>(value: &i64, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_i64(*value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Der Wert aus der echten accounts.json von 0.1.3.
    #[test]
    fn rfc3339_aus_version_0_1_3() {
        let v = json!("2026-08-13T19:27:34.391Z");
        let t = parse(&v, ExpiryFormat::Auto);
        assert!(t > 1_700_000_000, "muss ein plausibler Zeitpunkt sein, war {t}");
        // Gegenprobe gegen den bekannten Wert.
        let expected = chrono::DateTime::parse_from_rfc3339("2026-08-13T19:27:34.391Z")
            .unwrap().timestamp();
        assert_eq!(t, expected);
    }

    #[test]
    fn sekunden_und_millisekunden_werden_unterschieden() {
        let sekunden = 1_800_000_000i64;
        let millis = sekunden * 1000;
        assert_eq!(parse(&json!(sekunden), ExpiryFormat::Auto), sekunden);
        assert_eq!(parse(&json!(millis), ExpiryFormat::Auto), sekunden);
    }

    #[test]
    fn erzwungenes_format_schlaegt_die_erkennung() {
        let wert = json!(1_800_000_000i64);
        // Als Millisekunden gelesen ergibt derselbe Wert etwas anderes.
        assert_eq!(parse(&wert, ExpiryFormat::UnixMillis), 1_800_000);
        assert_eq!(parse(&wert, ExpiryFormat::UnixSeconds), 1_800_000_000);
    }

    #[test]
    fn zahl_in_anfuehrungszeichen() {
        assert_eq!(parse(&json!("1800000000"), ExpiryFormat::Auto), 1_800_000_000);
    }

    #[test]
    fn unlesbares_ergibt_null_und_gilt_damit_als_abgelaufen() {
        assert_eq!(parse(&json!("völlig kaputt"), ExpiryFormat::Auto), 0);
        assert_eq!(parse(&json!(null), ExpiryFormat::Auto), 0);
        assert_eq!(parse(&json!(""), ExpiryFormat::Auto), 0);
    }

    #[test]
    fn rfc3339_erzwungen_ignoriert_zahlen() {
        // Wer RFC 3339 erzwingt, bekommt fuer eine blosse Zahl keinen Wert.
        assert_eq!(parse(&json!("1800000000"), ExpiryFormat::Rfc3339), 0);
    }
}
