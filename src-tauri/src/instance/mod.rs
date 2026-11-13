//! Instanzen: eine Minecraft-Version, ein Loader, ein Satz Mods.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

/// Feldnamen unveraendert aus 0.1.3. Neue Felder sind optional, damit
/// eine bestehende `instance.json` unveraendert eingelesen wird.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct InstanceConfig {
    pub id: String,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub name: String,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub mc_version: String,
    /// `vanilla`, `buildmc`, `fabric`, `forge`, `neoforge`, `quilt`.
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub loader: String,
    pub fabric_version: Option<String>,
    pub buildmc_mod_version: Option<String>,

    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub mods: Vec<String>,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub resource_packs: Vec<String>,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub shader_packs: Vec<String>,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub jvm_args: Vec<String>,

    pub min_ram: u32,
    pub max_ram: u32,
    pub use_custom_ui: bool,

    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub created_at: String,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub last_played: String,

    // --- neu in 0.2 ---
    pub group: Option<String>,
    pub icon: Option<String>,
    pub accent: Option<String>,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub notes: String,
    pub locked: bool,
    pub play_seconds: u64,
    pub launch_count: u32,
    pub java_path_override: Option<String>,
}

impl Default for InstanceConfig {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            mc_version: String::new(),
            loader: "vanilla".into(),
            fabric_version: None,
            buildmc_mod_version: None,
            mods: Vec::new(),
            resource_packs: Vec::new(),
            shader_packs: Vec::new(),
            jvm_args: Vec::new(),
            min_ram: 2048,
            max_ram: 4096,
            use_custom_ui: false,
            created_at: String::new(),
            last_played: String::new(),
            group: None,
            icon: None,
            accent: None,
            notes: String::new(),
            locked: false,
            play_seconds: 0,
            launch_count: 0,
            java_path_override: None,
        }
    }
}

impl InstanceConfig {
    /// Die Versions-Id, die beim Start geladen wird.
    ///
    /// Bei Fabric ist das nicht die Minecraft-Version, sondern das
    /// Loader-Profil -- es erbt die Grundversion ueber `inheritsFrom`.
    /// Welches Profil beim Start geladen wird.
    ///
    /// Bei einem Loader ist das nicht die Minecraft-Version, sondern das
    /// Loader-Profil -- es erbt die Grundversion ueber `inheritsFrom`.
    ///
    /// **`buildmc` zaehlt dabei als Fabric.** Der BuildMC-Client ist eine
    /// Fabric-Mod; solche Instanzen tragen eine Fabric-Fassung und haben
    /// ein Fabric-Profil. Ihn wie Vanilla zu behandeln hiess, die Instanz
    /// ohne jede Mod zu starten -- das Spiel lief, aber es war nicht das,
    /// was in der Instanz steht.
    pub fn launch_version(&self) -> String {
        let Some(v) = self.fabric_version.as_deref().filter(|v| !v.is_empty()) else {
            return self.mc_version.clone();
        };
        match self.profile_kind() {
            Some(kind) => kind.version_id(v, &self.mc_version),
            None => self.mc_version.clone(),
        }
    }

    /// Welcher Loader das Profil stellt.
    pub fn profile_kind(&self) -> Option<crate::loader::LoaderKind> {
        match self.loader.as_str() {
            // Der BuildMC-Client setzt auf Fabric auf.
            "buildmc" => Some(crate::loader::LoaderKind::Fabric),
            other => crate::loader::LoaderKind::parse(other),
        }
    }

    /// Laeuft diese Instanz mit Mods?
    pub fn has_loader(&self) -> bool {
        self.profile_kind().is_some()
    }

    pub fn game_dir(&self) -> Result<std::path::PathBuf> {
        paths::instance_game_dir(&self.id)
    }
}

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Liest alle Instanzen. Ein unlesbarer Eintrag wird uebersprungen statt
/// die ganze Liste scheitern zu lassen.
pub fn list() -> Result<Vec<InstanceConfig>> {
    let root = paths::instances()?;
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
        match crate::store::read::<InstanceConfig>(&file) {
            Ok(c) => out.push(c),
            Err(e) => eprintln!("[instance] {} übersprungen: {e}", file.display()),
        }
    }

    out.sort_by(|a, b| b.last_played.cmp(&a.last_played));
    Ok(out)
}

pub fn load(id: &str) -> Result<InstanceConfig> {
    crate::store::read(&paths::instance_config(id)?)
        .map_err(|_| Error::not_found(format!("Instanz {id} nicht gefunden")))
}

pub fn save(config: &InstanceConfig) -> Result<()> {
    crate::store::write(&paths::instance_config(&config.id)?, config)
}

/// Legt eine Instanz samt Verzeichnissen an.
pub fn create(name: &str, mc_version: &str, loader: &str, use_custom_ui: bool) -> Result<InstanceConfig> {
    let cfg = crate::store::load_config();
    let (min_ram, max_ram) = cfg.ram();

    let instance = InstanceConfig {
        id: uuid::Uuid::new_v4().to_string(),
        name: name.trim().to_string(),
        mc_version: mc_version.to_string(),
        loader: loader.to_string(),
        min_ram,
        max_ram,
        use_custom_ui,
        created_at: now(),
        last_played: String::new(),
        ..Default::default()
    };

    let dir = paths::instance_dir(&instance.id)?;
    paths::ensure_dir(&dir)?;
    // Die Ordner, die Minecraft ohnehin anlegen wuerde -- so sind sie da,
    // bevor Mods hineingelegt werden.
    for sub in ["mods", "resourcepacks", "shaderpacks", "saves", "config"] {
        paths::ensure_dir(&instance.game_dir()?.join(sub))?;
    }

    save(&instance)?;
    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::InstanceConfig;

    #[test]
    fn quilt_startet_sein_eigenes_profil() {
        let i = InstanceConfig {
            mc_version: "1.21.4".into(),
            loader: "quilt".into(),
            fabric_version: Some("0.29.1".into()),
            ..Default::default()
        };
        assert_eq!(i.launch_version(), "quilt-loader-0.29.1-1.21.4");
    }

    #[test]
    fn leere_loader_fassung_startet_die_grundversion() {
        let i = InstanceConfig {
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            fabric_version: Some(String::new()),
            ..Default::default()
        };
        assert_eq!(i.launch_version(), "1.21.4");
    }

    #[test]
    fn fabric_startet_das_loader_profil() {
        let i = InstanceConfig {
            mc_version: "1.21.4".into(),
            loader: "fabric".into(),
            fabric_version: Some("0.19.5".into()),
            ..Default::default()
        };
        assert_eq!(i.launch_version(), "fabric-loader-0.19.5-1.21.4");
    }

    /// Aus dem echten Fall: eine BuildMC-Instanz startete als Vanilla und
    /// damit ohne jede Mod, obwohl das Fabric-Profil vorlag.
    #[test]
    fn buildmc_startet_das_fabric_profil() {
        let i = InstanceConfig {
            mc_version: "1.21.11".into(),
            loader: "buildmc".into(),
            fabric_version: Some("0.19.5".into()),
            ..Default::default()
        };
        assert_eq!(i.launch_version(), "fabric-loader-0.19.5-1.21.11");
        assert!(i.has_loader());
    }

    #[test]
    fn vanilla_startet_die_version_selbst() {
        let i = InstanceConfig {
            mc_version: "1.21.4".into(),
            loader: "vanilla".into(),
            ..Default::default()
        };
        assert_eq!(i.launch_version(), "1.21.4");
    }

    #[test]
    fn alte_instanz_bleibt_lesbar() {
        let raw = r#"{
            "id":"abc","name":"Test","mc_version":"1.16.5","loader":"buildmc",
            "fabric_version":null,"buildmc_mod_version":null,"mods":[],
            "resource_packs":[],"shader_packs":[],"jvm_args":[],
            "min_ram":2048,"max_ram":6144,"use_custom_ui":false,
            "created_at":"2026-05-02T10:00:00Z","last_played":"2026-09-27T18:12:00Z"
        }"#;
        let i: InstanceConfig = serde_json::from_str(raw).expect("muss lesbar sein");
        assert_eq!(i.name, "Test");
        assert_eq!(i.loader, "buildmc");
        assert_eq!(i.play_seconds, 0, "neues Feld bekommt seinen Standardwert");
    }
}

/* ---- Papierkorb und Vervielfaeltigen ----------------------------------- */

/// Verschiebt eine Instanz in den Papierkorb.
///
/// Bewusst verschieben statt loeschen: in einer Instanz stecken Welten,
/// und ein Fehlgriff waere sonst endgueltig.
pub fn trash(id: &str) -> Result<()> {
    if load(id)?.locked {
        return Err(Error::other("Diese Instanz ist gesperrt"));
    }
    let from = paths::instance_dir(id)?;
    let to = paths::trash()?.join(id);
    paths::ensure_parent(&to)?;
    if to.exists() {
        std::fs::remove_dir_all(&to)?;
    }
    move_dir(&from, &to)
}

pub fn restore(id: &str) -> Result<()> {
    let from = paths::trash()?.join(id);
    if !from.exists() {
        return Err(Error::not_found("Diese Instanz liegt nicht im Papierkorb"));
    }
    let to = paths::instance_dir(id)?;
    paths::ensure_parent(&to)?;
    move_dir(&from, &to)
}

/// Entfernt endgueltig.
pub fn purge(id: &str) -> Result<()> {
    let in_trash = paths::trash()?.join(id);
    if in_trash.exists() {
        std::fs::remove_dir_all(&in_trash)?;
        return Ok(());
    }
    let dir = paths::instance_dir(id)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

pub fn list_trashed() -> Result<Vec<InstanceConfig>> {
    let root = paths::trash()?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&root)? {
        let Ok(entry) = entry else { continue };
        let file = entry.path().join("instance.json");
        if let Ok(c) = crate::store::read::<InstanceConfig>(&file) {
            out.push(c);
        }
    }
    Ok(out)
}

/// Legt eine Kopie an.
///
/// Spielstaende werden mitgenommen, Protokolle und Zwischenspeicher nicht --
/// die gehoeren zum alten Lauf und waeren in der Kopie irrefuehrend.
pub fn duplicate(id: &str, new_name: &str) -> Result<InstanceConfig> {
    let source = load(id)?;
    let mut copy = source.clone();
    copy.id = uuid::Uuid::new_v4().to_string();
    copy.name = if new_name.trim().is_empty() {
        format!("{} (Kopie)", source.name)
    } else {
        new_name.trim().to_string()
    };
    copy.created_at = now();
    copy.last_played = String::new();
    copy.play_seconds = 0;
    copy.launch_count = 0;
    copy.locked = false;

    let from = paths::instance_dir(id)?;
    let to = paths::instance_dir(&copy.id)?;
    copy_dir(&from, &to, &["logs", ".fabric", ".mixin.out", "crash-reports", "downloads"])?;

    save(&copy)?;
    Ok(copy)
}

pub fn set_group(id: &str, group: Option<String>) -> Result<()> {
    let mut instance = load(id)?;
    instance.group = group.filter(|g| !g.trim().is_empty());
    save(&instance)
}

/// Verschiebt ein Verzeichnis, notfalls ueber Kopieren und Loeschen.
///
/// `rename` scheitert ueber Dateisystemgrenzen hinweg -- und genau das
/// kommt vor, wenn Instanzen auf einer anderen Platte liegen.
fn move_dir(from: &std::path::Path, to: &std::path::Path) -> Result<()> {
    match std::fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_dir(from, to, &[])?;
            std::fs::remove_dir_all(from)?;
            Ok(())
        }
    }
}

fn copy_dir(from: &std::path::Path, to: &std::path::Path, skip: &[&str]) -> Result<()> {
    paths::ensure_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if skip.contains(&name_str.as_ref()) {
            continue;
        }
        let target = to.join(&name);
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target, skip)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod verwaltung_tests {
    use super::*;

    fn temp(marke: &str) -> InstanceConfig {
        use std::sync::atomic::{AtomicU32, Ordering};
        static Z: AtomicU32 = AtomicU32::new(0);
        let id = format!("t-{}-{marke}-{}", std::process::id(), Z.fetch_add(1, Ordering::Relaxed));
        let c = InstanceConfig {
            id: id.clone(), name: format!("Test {marke}"),
            mc_version: "1.21.4".into(), loader: "vanilla".into(),
            created_at: now(), ..Default::default()
        };
        paths::ensure_dir(&paths::instance_game_dir(&id).unwrap()).unwrap();
        save(&c).unwrap();
        c
    }

    fn aufraeumen(id: &str) {
        let _ = std::fs::remove_dir_all(paths::instance_dir(id).unwrap());
        let _ = std::fs::remove_dir_all(paths::trash().unwrap().join(id));
    }

    #[test]
    fn papierkorb_und_zurueck() {
        let c = temp("papierkorb");
        trash(&c.id).unwrap();
        assert!(!paths::instance_dir(&c.id).unwrap().exists());
        assert!(list_trashed().unwrap().iter().any(|i| i.id == c.id));

        restore(&c.id).unwrap();
        assert!(paths::instance_dir(&c.id).unwrap().exists());
        aufraeumen(&c.id);
    }

    #[test]
    fn gesperrte_instanz_bleibt() {
        let mut c = temp("gesperrt");
        c.locked = true;
        save(&c).unwrap();
        assert!(trash(&c.id).is_err());
        aufraeumen(&c.id);
    }

    #[test]
    fn kopie_beginnt_bei_null() {
        let c = temp("kopie");
        let mut mit_zeit = c.clone();
        mit_zeit.play_seconds = 9999;
        mit_zeit.launch_count = 42;
        save(&mit_zeit).unwrap();

        let kopie = duplicate(&c.id, "").unwrap();
        assert_ne!(kopie.id, c.id);
        assert!(kopie.name.ends_with("(Kopie)"));
        assert_eq!(kopie.play_seconds, 0, "die Spielzeit gehört dem Original");
        assert_eq!(kopie.launch_count, 0);

        aufraeumen(&c.id);
        aufraeumen(&kopie.id);
    }

    #[test]
    fn gruppe_wird_gesetzt_und_geleert() {
        let c = temp("gruppe");
        set_group(&c.id, Some("Survival".into())).unwrap();
        assert_eq!(load(&c.id).unwrap().group.as_deref(), Some("Survival"));
        // Leerzeichen gelten als keine Gruppe.
        set_group(&c.id, Some("   ".into())).unwrap();
        assert_eq!(load(&c.id).unwrap().group, None);
        aufraeumen(&c.id);
    }
}
