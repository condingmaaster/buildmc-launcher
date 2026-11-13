//! Bedrock-Instanzen.
//!
//! Bewusst kein Gegenstueck zu `auth::Account`: welches Google-Konto bei
//! `mcpelauncher-ui-qt` verknuepft ist, bleibt dessen eigene Sache (siehe
//! `detect::google_account_linked` -- reine Existenzpruefung, keine
//! Entschluesselung). Eine Instanz hier ist nur ein Zeiger auf eine bereits
//! heruntergeladene, echte Bedrock-Version plus eigene Speicherstaende.

pub mod detect;
pub mod launch;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BedrockInstance {
    pub id: String,
    pub name: String,
    /// Verzeichnisname unter `mcpelauncher/versions/`, z.B. `1.21.50.07`.
    pub version_dir_name: String,
    pub created_at: String,
    pub last_played: String,
    pub play_seconds: u64,
    pub launch_count: u32,
}

impl Default for BedrockInstance {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            version_dir_name: String::new(),
            created_at: String::new(),
            last_played: String::new(),
            play_seconds: 0,
            launch_count: 0,
        }
    }
}

impl BedrockInstance {
    fn dir(&self) -> Result<std::path::PathBuf> {
        paths::bedrock_instance_dir(&self.id)
    }

    fn config_file(&self) -> Result<std::path::PathBuf> {
        Ok(self.dir()?.join("instance.json"))
    }

    /// Eigene Speicherstaende/Einstellungen -- getrennt von dem, was
    /// `mcpelauncher-ui-qt` fuer seine eigenen Standardprofile nutzt, damit
    /// ein direkter Start ueber dessen eigenen Spielen-Knopf sich nicht mit
    /// einer BuildMC-Instanz in die Quere kommt.
    pub fn data_dir(&self) -> Result<std::path::PathBuf> {
        Ok(self.dir()?.join("data"))
    }

    pub fn cache_dir(&self) -> Result<std::path::PathBuf> {
        Ok(self.dir()?.join("cache"))
    }

    /// Wo die Spieldateien/Assets dieser Version liegen -- geteilt mit
    /// `mcpelauncher-ui-qt` und jeder anderen Instanz derselben Version,
    /// absichtlich (mehrere Instanzen einer Version teilen sich die Assets,
    /// genau wie bei Java `versions/<id>/`).
    pub fn game_dir(&self) -> Result<std::path::PathBuf> {
        Ok(detect::versions_dir()?.join(&self.version_dir_name))
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Liest alle Bedrock-Instanzen. Ein unlesbarer Eintrag wird uebersprungen.
pub fn list() -> Result<Vec<BedrockInstance>> {
    let root = paths::bedrock_instances()?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let Ok(entry) = entry else { continue };
        let file = entry.path().join("instance.json");
        if !file.exists() {
            continue;
        }
        match crate::store::read::<BedrockInstance>(&file) {
            Ok(c) => out.push(c),
            Err(e) => eprintln!("[bedrock] {} übersprungen: {e}", file.display()),
        }
    }

    out.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    Ok(out)
}

pub fn load(id: &str) -> Result<BedrockInstance> {
    let path = paths::bedrock_instance_dir(id)?.join("instance.json");
    crate::store::read(&path).map_err(|_| Error::not_found(format!("Bedrock-Instanz {id} nicht gefunden")))
}

pub fn save(instance: &BedrockInstance) -> Result<()> {
    crate::store::write(&instance.config_file()?, instance)
}

/// Legt eine Instanz an, die auf eine bereits erkannte Version zeigt.
pub fn create(name: &str, version_dir_name: &str) -> Result<BedrockInstance> {
    let available = detect::list_versions()?;
    if !available.iter().any(|v| v.dir_name == version_dir_name) {
        return Err(Error::not_found(format!(
            "Version {version_dir_name} wurde nicht unter den heruntergeladenen Bedrock-Versionen gefunden"
        )));
    }

    let instance = BedrockInstance {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        version_dir_name: version_dir_name.to_string(),
        created_at: now(),
        last_played: String::new(),
        play_seconds: 0,
        launch_count: 0,
    };

    paths::ensure_dir(&instance.data_dir()?)?;
    paths::ensure_dir(&instance.cache_dir()?)?;
    save(&instance)?;
    Ok(instance)
}

pub fn record_playtime(instance_id: &str, seconds: u64) {
    let Ok(mut instance) = load(instance_id) else { return };
    instance.play_seconds += seconds;
    let _ = save(&instance);
}
