//! Regelauswertung fuer Bibliotheken und Startargumente.
//!
//! Jede Regel erlaubt (`allow`) oder verbietet (`disallow`) etwas, sofern
//! ihre Bedingung zutrifft. Mojangs Auswertung ist dabei eigen: gibt es
//! ueberhaupt Regeln, gilt etwas nur dann, wenn zuletzt ein `allow`
//! gegriffen hat. Eine Regel ohne Bedingung trifft immer zu.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Rule {
    pub action: String,
    #[serde(default)]
    pub os: Option<OsRule>,
    #[serde(default)]
    pub features: HashMap<String, bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OsRule {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub arch: Option<String>,
    /// Regulaerer Ausdruck auf die Systemversion. Wird bewusst ignoriert:
    /// er betrifft nur Windows 10 gegen aeltere Ausgaben und ist fuer die
    /// Auswahl der Bibliotheken ohne Bedeutung.
    #[serde(default)]
    pub version: Option<String>,
}

/// Merkmale, die Mojang in den Startargumenten abfragt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Feature {
    IsDemoUser,
    HasCustomResolution,
    HasQuickPlaysSupport,
    IsQuickPlaySingleplayer,
    IsQuickPlayMultiplayer,
    IsQuickPlayRealms,
}

impl Feature {
    pub fn key(self) -> &'static str {
        match self {
            Feature::IsDemoUser => "is_demo_user",
            Feature::HasCustomResolution => "has_custom_resolution",
            Feature::HasQuickPlaysSupport => "has_quick_plays_support",
            Feature::IsQuickPlaySingleplayer => "is_quick_play_singleplayer",
            Feature::IsQuickPlayMultiplayer => "is_quick_play_multiplayer",
            Feature::IsQuickPlayRealms => "is_quick_play_realms",
        }
    }
}

/// Das aktuelle System samt der gesetzten Merkmale.
#[derive(Debug, Clone)]
pub struct RuleContext {
    pub os_name: &'static str,
    pub os_arch: &'static str,
    pub features: HashMap<String, bool>,
}

impl RuleContext {
    pub fn current() -> Self {
        Self {
            os_name: current_os(),
            os_arch: current_arch(),
            features: HashMap::new(),
        }
    }

    pub fn with(mut self, f: Feature, on: bool) -> Self {
        self.features.insert(f.key().to_string(), on);
        self
    }

    fn feature(&self, key: &str) -> bool {
        self.features.get(key).copied().unwrap_or(false)
    }

    fn matches(&self, rule: &Rule) -> bool {
        if let Some(os) = &rule.os {
            if let Some(name) = &os.name {
                if name != self.os_name {
                    return false;
                }
            }
            if let Some(arch) = &os.arch {
                if arch != self.os_arch {
                    return false;
                }
            }
        }
        for (key, want) in &rule.features {
            if self.feature(key) != *want {
                return false;
            }
        }
        true
    }

    /// Wertet eine Regelliste aus. Ohne Regeln gilt etwas immer.
    pub fn allows(&self, rules: &[Rule]) -> bool {
        if rules.is_empty() {
            return true;
        }
        let mut allowed = false;
        for rule in rules {
            if !self.matches(rule) {
                continue;
            }
            allowed = rule.action == "allow";
        }
        allowed
    }
}

/// Mojangs Bezeichnungen, nicht die von Rust.
pub fn current_os() -> &'static str {
    match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "osx",
        other => other, // "linux"
    }
}

/// Mojang unterscheidet x86, x86_64 und arm64.
pub fn current_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x86_64",
        "x86" => "x86",
        "aarch64" => "arm64",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(action: &str, os: Option<&str>) -> Rule {
        Rule {
            action: action.into(),
            os: os.map(|n| OsRule { name: Some(n.into()), arch: None, version: None }),
            features: HashMap::new(),
        }
    }

    fn ctx(os: &'static str) -> RuleContext {
        RuleContext { os_name: os, os_arch: "x86_64", features: HashMap::new() }
    }

    #[test]
    fn ohne_regeln_immer_erlaubt() {
        assert!(ctx("linux").allows(&[]));
    }

    #[test]
    fn allow_nur_fuer_passendes_system() {
        let rules = vec![rule("allow", Some("osx"))];
        assert!(ctx("osx").allows(&rules));
        assert!(!ctx("linux").allows(&rules));
    }

    /// Das verbreitete Muster: alles erlauben, ein System ausnehmen.
    #[test]
    fn allow_dann_disallow() {
        let rules = vec![rule("allow", None), rule("disallow", Some("osx"))];
        assert!(ctx("linux").allows(&rules));
        assert!(!ctx("osx").allows(&rules));
    }

    #[test]
    fn merkmale_muessen_uebereinstimmen() {
        let mut r = rule("allow", None);
        r.features.insert("is_demo_user".into(), true);
        let rules = vec![r];
        assert!(!ctx("linux").allows(&rules));
        assert!(ctx("linux").with(Feature::IsDemoUser, true).allows(&rules));
    }
}
