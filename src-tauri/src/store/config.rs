//! Die Einstellungen des Launchers.
//!
//! Feldnamen wie in 0.1.3. Neue Felder sind optional mit Standardwert,
//! damit eine aeltere `config.json` unveraendert eingelesen werden kann.

use serde::{Deserialize, Serialize};

fn d_min_ram() -> u32 { 2048 }
fn d_max_ram() -> u32 { 4096 }
fn d_downloads() -> usize { 8 }
fn d_shade() -> u8 { 8 }
fn d_accent() -> String { "#7c5cff".into() }
fn d_theme() -> String { "dark".into() }
fn d_egg_freq() -> u8 { 3 }
fn d_snapshot_keep() -> u8 { 5 }
fn d_language() -> String { "de".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    // --- Darstellung ---
    pub theme: String,
    pub accent_color: String,
    pub background_shade: u8,

    // --- Spiel ---
    pub min_ram: u32,
    pub max_ram: u32,
    pub java_path: String,
    pub use_zgc: bool,

    // --- Downloads ---
    pub concurrent_downloads: usize,
    pub verify_sha1: bool,

    // --- Start ---
    pub close_on_launch: bool,
    pub high_process_priority: bool,
    pub discord_rpc: bool,
    pub performance_mode: bool,

    // --- Aktualisierung ---
    pub auto_update: bool,
    pub skipped_update_version: Option<String>,

    // --- Spielereien aus 0.1.3 ---
    pub easter_eggs_enabled: bool,
    pub easter_egg_frequency: u8,

    // --- neu in 0.2 ---
    pub language: String,
    pub auto_snapshot: bool,
    pub snapshot_keep: u8,
    pub mod_update_check: bool,
    pub curseforge_enabled: bool,
    pub telemetry_overlay: bool,

    /// Wie der Ablaufzeitpunkt eines Tokens in `accounts.json` gelesen wird.
    /// `Auto` erkennt das Format am Wert und ist fuer alle bekannten
    /// Launcher richtig; die uebrigen Werte sind fuer Sonderfaelle da.
    pub account_expiry_format: crate::auth::ExpiryFormat,

    /// Denkaufwand anfordern.
    ///
    /// Das Modell denkt gruendlicher, die Antwort dauert laenger. Nicht
    /// jedes Modell nimmt den Parameter an -- lehnt es ab, laesst der
    /// Launcher ihn kuenftig von allein weg.
    pub ai_reasoning: bool,

    /// Werkzeuge bei der Absturzanalyse zulassen.
    ///
    /// Greift nur bei Abstuerzen, die keine Regel abdeckt: Websuche fuer
    /// die Fehlersignatur, Codeausfuehrung zum Zerlegen der Stapelspur.
    pub ai_tools: bool,

    /// Modell fuer die Assistenzfunktionen. Leer bedeutet Voreinstellung.
    pub ai_model: String,
    /// Ersatzmodell, falls das erste nicht erreichbar ist. Es wird nie von
    /// allein genommen -- der Nutzer muss zustimmen.
    pub ai_fallback_model: String,

    /// Web-Recherche beim Zuschneiden des Assistenten-Fragebogens.
    ///
    /// Absichtlich standardmaessig aus: kostet spuerbar mehr Zeit und
    /// Kontingent als der reine Zuschnitt. Im Assistenten selbst ist der
    /// Schalter sichtbar und aendert diesen Wert nur fuer kuenftige Laeufe,
    /// nicht rueckwirkend.
    pub assistant_web_research: bool,

    /// Wie streng ein Sicherheitsfund vor dem Start behandelt wird.
    #[serde(default)]
    pub security_mode: crate::security::Mode,
    /// Pflichtpruefung vor jedem Start ein- oder ausschalten -- ganz
    /// abschaltbar fuer Nutzer ohne Internetzugang beim Spielen.
    #[serde(default = "d_true")]
    pub security_auto_scan: bool,

    /// Welche Spielart beim naechsten Start voreingestellt ist -- der
    /// Editions-Schalter in der Leiste merkt sich seine Wahl hier.
    /// "java" oder "bedrock".
    #[serde(default = "d_edition")]
    pub default_edition: String,
}

fn d_edition() -> String { "java".into() }

fn d_true() -> bool { true }

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: d_theme(),
            accent_color: d_accent(),
            background_shade: d_shade(),
            min_ram: d_min_ram(),
            max_ram: d_max_ram(),
            java_path: String::new(),
            use_zgc: true,
            concurrent_downloads: d_downloads(),
            verify_sha1: true,
            close_on_launch: false,
            high_process_priority: false,
            discord_rpc: true,
            performance_mode: false,
            auto_update: true,
            skipped_update_version: None,
            easter_eggs_enabled: true,
            easter_egg_frequency: d_egg_freq(),
            language: d_language(),
            auto_snapshot: true,
            snapshot_keep: d_snapshot_keep(),
            mod_update_check: true,
            curseforge_enabled: true,
            telemetry_overlay: true,
            ai_reasoning: true,
            ai_tools: true,
            account_expiry_format: Default::default(),
            ai_model: crate::ai::DEFAULT_MODEL.to_string(),
            ai_fallback_model: crate::ai::client::FALLBACK_MODEL.to_string(),
            assistant_web_research: false,
            security_mode: crate::security::Mode::default(),
            security_auto_scan: true,
            default_edition: d_edition(),
        }
    }
}

impl Config {
    /// Arbeitsspeichergrenzen einer Instanz, die keine eigenen setzt.
    pub fn ram(&self) -> (u32, u32) {
        let max = self.max_ram.max(512);
        (self.min_ram.min(max), max)
    }

    pub fn model(&self) -> &str {
        if self.ai_model.trim().is_empty() {
            crate::ai::DEFAULT_MODEL
        } else {
            &self.ai_model
        }
    }

    pub fn fallback_model(&self) -> &str {
        if self.ai_fallback_model.trim().is_empty() {
            crate::ai::client::FALLBACK_MODEL
        } else {
            &self.ai_fallback_model
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Config;

    #[test]
    fn alte_config_bleibt_lesbar() {
        // Genau die Felder aus 0.1.3, ohne die neuen.
        let raw = r##"{
            "theme":"dark","max_ram":6144,"min_ram":2048,"use_zgc":true,
            "java_path":"","concurrent_downloads":8,"verify_sha1":true,
            "close_on_launch":false,"auto_update":true,"discord_rpc":true,
            "accent_color":"#7c5cff","background_shade":8,
            "skipped_update_version":null,"easter_eggs_enabled":true,
            "easter_egg_frequency":3,"performance_mode":false,
            "high_process_priority":false
        }"##;
        let c: Config = serde_json::from_str(raw).expect("muss lesbar sein");
        assert_eq!(c.max_ram, 6144);
        assert_eq!(c.accent_color, "#7c5cff");
        // Neue Felder bekommen ihren Standardwert.
        assert!(c.auto_snapshot);
        assert_eq!(c.language, "de");
    }

    #[test]
    fn unbekannte_felder_stoeren_nicht() {
        // Die AI-Felder aus 0.1.3, die es nicht mehr gibt.
        let raw = r#"{"theme":"dark","ai_enabled":true,"ai_ollama_url":"http://x"}"#;
        let c: Config = serde_json::from_str(raw).expect("muss lesbar sein");
        assert_eq!(c.theme, "dark");
    }

    #[test]
    fn ram_grenzen_bleiben_sinnvoll() {
        let c = Config { min_ram: 8192, max_ram: 4096, ..Default::default() };
        let (min, max) = c.ram();
        assert!(min <= max, "Minimum darf das Maximum nicht überschreiten");
    }
}
