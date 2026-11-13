//! Rust-Editionen: Minecraft: Java Edition 26.3, in Rust neu geschrieben.
//!
//! Zwei Varianten teilen sich diesen Code, getrennt in Ordnern, Programmen und Instanzen:
//!
//! - **Horizon** (Option 3): der unveraenderte Horizon-Quellcode (`rust-edition/horizon/`,
//!   MIT, siehe `rust-edition/VENDORED.md`).
//! - **BuildMC Horizon Hybrid, BHH** (Option 4): BuildMCs Fork (`bhh/`, Branch `bhh`) mit
//!   Fabric-Bruecke: eine eingebettete JVM fuehrt Fabric-Mods aus, Horizon bleibt die Engine.
//!
//! BuildMC baut beide selbst und startet sie ueber Horizons Launch-API:
//! 1. `engine`  -- Quellcode finden, bauen, Stand pruefen
//! 2. `prepare` -- aus der eigenen 26.3 die Spieldaten erzeugen (nur fuer ein Microsoft-Konto,
//!    dem Java Edition gehoert)
//! 3. `launch`  -- starten, Konto ueber stdin, nie ueber die Kommandozeile
//! 4. `fabric`  -- (nur BHH) Fabric-Mods einer Instanz und ihre Kompatibilitaet

pub mod engine;
pub mod fabric;
pub mod launch;
pub mod prepare;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

/// Die einzige Version, die Horizon spielt.
pub const MINECRAFT_VERSION: &str = "26.3";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Variant {
    Horizon,
    Bhh,
}

impl Variant {
    /// Aus dem Befehl der Oberflaeche; ohne Angabe Horizon (Option 3).
    pub fn parse(v: Option<&str>) -> Self {
        match v {
            Some("bhh") => Variant::Bhh,
            _ => Variant::Horizon,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn label(self) -> &'static str {
        match self {
            Variant::Horizon => "Horizon",
            Variant::Bhh => "BuildMC Horizon Hybrid",
        }
    }

    /// Das Spielprogramm.
    pub fn client(self) -> &'static str {
        match self {
            Variant::Horizon => "horizon-client",
            Variant::Bhh => "bhh-client",
        }
    }

    pub fn root(self) -> Result<PathBuf> {
        Ok(paths::root()?.join(match self {
            Variant::Horizon => "rust-edition",
            Variant::Bhh => "bhh",
        }))
    }

    /// Gebaute Programme (Spiel, `horizon-prepare`, bei BHH `bhh-api.jar`).
    pub fn bin_dir(self) -> Result<PathBuf> {
        Ok(self.root()?.join("bin"))
    }

    /// Cargo-Zielordner -- ausserhalb des Quellcodes.
    pub fn build_dir(self) -> Result<PathBuf> {
        Ok(self.root()?.join("build"))
    }

    /// Vorbereitete Spieldaten. Je Variante eigene: BHH folgt einem anderen Horizon-Stand.
    pub fn prepared_dir(self) -> Result<PathBuf> {
        Ok(self.root()?.join("prepared").join(MINECRAFT_VERSION))
    }

    pub fn instances_dir(self) -> Result<PathBuf> {
        Ok(self.root()?.join("instances"))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RustInstance {
    pub id: String,
    pub name: String,
    pub variant: Variant,
    pub created_at: String,
    pub last_played: String,
    pub play_seconds: u64,
    pub launch_count: u32,
    /// Server, dem beim Start direkt beigetreten wird (leer = Titelbild).
    pub quick_server: String,
    /// Welt aus `saves/`, die beim Start geoeffnet wird (leer = Titelbild).
    pub quick_world: String,
    pub fullscreen: bool,
    /// Nur BHH: Java-Heap fuer Fabric-Mods und Fassaden (keine Welt), in MB.
    pub jvm_heap_mb: u32,
}

impl Default for RustInstance {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            variant: Variant::Horizon,
            created_at: String::new(),
            last_played: String::new(),
            play_seconds: 0,
            launch_count: 0,
            quick_server: String::new(),
            quick_world: String::new(),
            fullscreen: false,
            jvm_heap_mb: 512,
        }
    }
}

impl RustInstance {
    pub fn dir(&self) -> Result<PathBuf> {
        paths::safe_join(&self.variant.instances_dir()?, &self.id)
    }

    /// Welten, `options.txt`, Ressourcenpakete (`--game-dir`).
    pub fn game_dir(&self) -> Result<PathBuf> {
        Ok(self.dir()?.join("game"))
    }

    /// Protokolle und Absturzberichte (`--home`).
    pub fn home_dir(&self) -> Result<PathBuf> {
        Ok(self.dir()?.join("home"))
    }

    /// Nur BHH: die Fabric-Mods dieser Instanz.
    pub fn fabric_mods_dir(&self) -> Result<PathBuf> {
        Ok(self.dir()?.join("fabric-mods"))
    }

    fn config_file(&self) -> Result<PathBuf> {
        Ok(self.dir()?.join("instance.json"))
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Alle Instanzen einer Variante, zuletzt gespielte zuerst. Unlesbares wird uebersprungen.
pub fn list(v: Variant) -> Result<Vec<RustInstance>> {
    let root = v.instances_dir()?;
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
        match crate::store::read::<RustInstance>(&file) {
            Ok(mut i) => {
                i.variant = v;
                out.push(i)
            }
            Err(e) => eprintln!("[rust] {} übersprungen: {e}", file.display()),
        }
    }
    out.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    Ok(out)
}

pub fn load(v: Variant, id: &str) -> Result<RustInstance> {
    let path = paths::safe_join(&v.instances_dir()?, id)?.join("instance.json");
    let mut i: RustInstance = crate::store::read(&path)
        .map_err(|_| Error::not_found(format!("{}-Instanz {id} nicht gefunden", v.label())))?;
    i.variant = v;
    Ok(i)
}

pub fn save(instance: &RustInstance) -> Result<()> {
    crate::store::write(&instance.config_file()?, instance)
}

pub fn create(v: Variant, name: &str, quick_server: &str) -> Result<RustInstance> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::other("Bitte einen Namen angeben"));
    }
    let instance = RustInstance {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.to_string(),
        variant: v,
        created_at: now(),
        quick_server: quick_server.trim().to_string(),
        ..Default::default()
    };
    paths::ensure_dir(&instance.game_dir()?)?;
    paths::ensure_dir(&instance.home_dir()?)?;
    if v == Variant::Bhh {
        paths::ensure_dir(&instance.fabric_mods_dir()?)?;
    }
    save(&instance)?;
    Ok(instance)
}

pub struct Aenderung<'a> {
    pub name: Option<&'a str>,
    pub quick_server: Option<&'a str>,
    pub quick_world: Option<&'a str>,
    pub fullscreen: Option<bool>,
    pub jvm_heap_mb: Option<u32>,
}

/// Einstellungen einer Instanz aendern.
pub fn update(v: Variant, id: &str, a: Aenderung<'_>) -> Result<RustInstance> {
    let mut i = load(v, id)?;
    if let Some(n) = a.name.map(str::trim).filter(|n| !n.is_empty()) {
        i.name = n.to_string();
    }
    if let Some(s) = a.quick_server {
        i.quick_server = s.trim().to_string();
    }
    if let Some(w) = a.quick_world {
        // Horizon nimmt nur einen Ordnernamen, keinen Pfad.
        if !w.is_empty() && !paths::is_bare_name(w) {
            return Err(Error::other("Welt: nur der Ordnername aus saves/, kein Pfad"));
        }
        i.quick_world = w.to_string();
    }
    if let Some(f) = a.fullscreen {
        i.fullscreen = f;
    }
    if let Some(mb) = a.jvm_heap_mb {
        i.jvm_heap_mb = mb.clamp(128, 8192);
    }
    save(&i)?;
    Ok(i)
}

/// Loescht eine Instanz samt Welten -- nur wenn sie nicht laeuft.
pub fn delete(v: Variant, id: &str) -> Result<()> {
    if launch::is_running(id) {
        return Err(Error::other("Die Instanz läuft noch"));
    }
    let i = load(v, id)?;
    let dir = i.dir()?;
    if dir.starts_with(v.instances_dir()?) && dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

pub fn record_playtime(v: Variant, instance_id: &str, seconds: u64) {
    let Ok(mut i) = load(v, instance_id) else { return };
    i.play_seconds += seconds;
    let _ = save(&i);
}
