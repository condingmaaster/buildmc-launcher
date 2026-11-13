//! Startargumente zusammensetzen.
//!
//! Ab 1.13 liegen die Argumente als Liste vor, deren Eintraege entweder
//! eine Zeichenkette oder ein Objekt mit Regeln sind. Davor gab es nur
//! eine einzelne Zeile in `minecraftArguments`. Beide Formen muessen
//! bedient werden, und in beiden stehen Platzhalter der Form `${name}`.

use std::collections::HashMap;

use crate::manifest::rules::{Rule, RuleContext};

/// Sammelt die Werte, mit denen Platzhalter ersetzt werden.
#[derive(Debug, Default, Clone)]
pub struct Substitutions(HashMap<String, String>);

impl Substitutions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(mut self, key: &str, value: impl Into<String>) -> Self {
        self.0.insert(key.to_string(), value.into());
        self
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(String::as_str)
    }

    /// Ersetzt jedes `${…}`. Unbekannte Platzhalter bleiben unveraendert
    /// stehen -- so faellt im Protokoll auf, was fehlt, statt dass ein
    /// leeres Argument uebergeben wird.
    pub fn apply(&self, input: &str) -> String {
        let mut out = String::with_capacity(input.len());
        let mut rest = input;

        while let Some(start) = rest.find("${") {
            out.push_str(&rest[..start]);
            let after = &rest[start + 2..];
            match after.find('}') {
                Some(end) => {
                    let key = &after[..end];
                    match self.0.get(key) {
                        Some(value) => out.push_str(value),
                        None => {
                            out.push_str("${");
                            out.push_str(key);
                            out.push('}');
                        }
                    }
                    rest = &after[end + 1..];
                }
                None => {
                    out.push_str("${");
                    rest = after;
                    break;
                }
            }
        }
        out.push_str(rest);
        out
    }
}

/// Wertet eine Argumentliste aus dem neuen Format aus.
pub fn expand(entries: &[serde_json::Value], ctx: &RuleContext, subs: &Substitutions) -> Vec<String> {
    let mut out = Vec::new();

    for entry in entries {
        match entry {
            serde_json::Value::String(s) => out.push(subs.apply(s)),

            serde_json::Value::Object(map) => {
                let rules: Vec<Rule> = map
                    .get("rules")
                    .and_then(|r| serde_json::from_value(r.clone()).ok())
                    .unwrap_or_default();

                if !ctx.allows(&rules) {
                    continue;
                }

                match map.get("value") {
                    Some(serde_json::Value::String(s)) => out.push(subs.apply(s)),
                    Some(serde_json::Value::Array(items)) => {
                        for item in items {
                            if let Some(s) = item.as_str() {
                                out.push(subs.apply(s));
                            }
                        }
                    }
                    _ => {}
                }
            }

            _ => {}
        }
    }

    out
}

/// Zerlegt die alte Einzelzeile und ersetzt die Platzhalter.
pub fn expand_legacy(line: &str, subs: &Substitutions) -> Vec<String> {
    line.split_whitespace().map(|token| subs.apply(token)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::rules::RuleContext;

    fn ctx() -> RuleContext {
        RuleContext { os_name: "linux", os_arch: "x86_64", features: HashMap::new() }
    }

    #[test]
    fn platzhalter_werden_ersetzt() {
        let s = Substitutions::new()
            .set("auth_player_name", "Jona")
            .set("version_name", "1.21.4");
        assert_eq!(s.apply("--user ${auth_player_name}"), "--user Jona");
        assert_eq!(s.apply("${version_name}-client"), "1.21.4-client");
    }

    #[test]
    fn unbekannter_platzhalter_bleibt_stehen() {
        let s = Substitutions::new();
        assert_eq!(s.apply("${gibt_es_nicht}"), "${gibt_es_nicht}");
    }

    #[test]
    fn regeln_filtern_argumente() {
        let entries: Vec<serde_json::Value> = serde_json::from_str(
            r#"[
                "--immer",
                { "rules": [{ "action": "allow", "os": { "name": "osx" } }],
                  "value": "--nur-mac" },
                { "rules": [{ "action": "allow", "os": { "name": "linux" } }],
                  "value": ["--a", "--b"] }
            ]"#,
        )
        .unwrap();

        let got = expand(&entries, &ctx(), &Substitutions::new());
        assert_eq!(got, vec!["--immer", "--a", "--b"]);
    }

    #[test]
    fn alte_argumentzeile() {
        let s = Substitutions::new().set("auth_player_name", "Jona");
        let got = expand_legacy("--username ${auth_player_name} --version 1.8.9", &s);
        assert_eq!(got, vec!["--username", "Jona", "--version", "1.8.9"]);
    }
}
