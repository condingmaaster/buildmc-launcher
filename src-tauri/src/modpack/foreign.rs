//! Instanzen aus anderen Launchern übernehmen.
//!
//! Prism und MultiMC legen je Instanz eine `instance.cfg` mit
//! `Schlüssel=Wert` an und daneben ein `mmc-pack.json`, das Minecraft- und
//! Loader-Fassung als Liste von Bestandteilen führt. Das Spielverzeichnis
//! heisst dort `.minecraft` statt `minecraft`.
//!
//! Die Modrinth-App legt `profile.json` an, CurseForge `minecraftinstance.json`.
//!
//! Übernommen wird **kopiert, nicht verschoben**: der andere Launcher soll
//! danach unverändert weiterlaufen.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct ForeignLauncher {
    pub launcher: String,
    pub path: String,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignInstance {
    pub launcher: String,
    pub name: String,
    pub path: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub mod_count: usize,
}

/// Wo andere Launcher üblicherweise liegen.
fn candidates() -> Vec<(&'static str, PathBuf)> {
    let Some(data) = dirs::data_dir() else { return Vec::new() };
    let home = dirs::home_dir().unwrap_or_default();

    vec![
        ("Prism Launcher", data.join("PrismLauncher").join("instances")),
        ("MultiMC", home.join(".local/share/multimc/instances")),
        ("Modrinth App", data.join("com.modrinth.theseus").join("profiles")),
        ("CurseForge", home.join("curseforge").join("minecraft").join("Instances")),
    ]
}

/// Sucht vorhandene Installationen.
pub fn detect() -> Vec<ForeignLauncher> {
    candidates()
        .into_iter()
        .filter_map(|(name, dir)| {
            if !dir.is_dir() {
                return None;
            }
            let count = std::fs::read_dir(&dir)
                .map(|it| it.filter_map(|e| e.ok()).filter(|e| e.path().is_dir()).count())
                .unwrap_or(0);
            (count > 0).then(|| ForeignLauncher {
                launcher: name.to_string(),
                path: dir.display().to_string(),
                count,
            })
        })
        .collect()
}

/* ---- Prism und MultiMC --------------------------------------------------- */

#[derive(Debug, Deserialize)]
pub struct MmcPack {
    #[serde(default)]
    pub components: Vec<MmcComponent>,
}

#[derive(Debug, Deserialize)]
pub struct MmcComponent {
    pub uid: String,
    #[serde(default)]
    pub version: String,
}

/// Liest `Schlüssel=Wert`-Dateien. Zeilen ohne Gleichheitszeichen und
/// Kommentare werden übergangen.
pub fn parse_cfg(raw: &str) -> HashMap<String, String> {
    raw.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with('['))
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
        .collect()
}

/// Ordnet die Bestandteilskennungen einem Loader zu.
pub fn loader_from_components(pack: &MmcPack) -> (String, Option<String>, String) {
    let mut mc_version = String::new();
    let mut loader = "vanilla".to_string();
    let mut loader_version = None;

    for c in &pack.components {
        match c.uid.as_str() {
            "net.minecraft" => mc_version = c.version.clone(),
            "net.fabricmc.fabric-loader" => {
                loader = "fabric".into();
                loader_version = Some(c.version.clone());
            }
            "org.quiltmc.quilt-loader" => {
                loader = "quilt".into();
                loader_version = Some(c.version.clone());
            }
            "net.neoforged" => {
                loader = "neoforge".into();
                loader_version = Some(c.version.clone());
            }
            "net.minecraftforge" => {
                loader = "forge".into();
                loader_version = Some(c.version.clone());
            }
            _ => {}
        }
    }

    (loader, loader_version, mc_version)
}

/// Liest eine fremde Instanz, ohne etwas zu schreiben.
pub fn inspect(dir: &Path) -> Result<ForeignInstance> {
    if !dir.is_dir() {
        return Err(Error::not_found(format!("{} ist kein Ordner", dir.display())));
    }

    let cfg_path = dir.join("instance.cfg");
    let pack_path = dir.join("mmc-pack.json");

    if !cfg_path.exists() || !pack_path.exists() {
        return Err(Error::other(
            "Dieser Ordner sieht nicht nach einer Prism- oder MultiMC-Instanz aus.",
        ));
    }

    let cfg = parse_cfg(&std::fs::read_to_string(&cfg_path)?);
    let pack: MmcPack = serde_json::from_slice(&std::fs::read(&pack_path)?)?;
    let (loader, loader_version, mc_version) = loader_from_components(&pack);

    let game_dir = game_dir_of(dir);
    let mod_count = std::fs::read_dir(game_dir.join("mods"))
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter(|e| e.file_name().to_string_lossy().ends_with(".jar"))
                .count()
        })
        .unwrap_or(0);

    Ok(ForeignInstance {
        launcher: "Prism / MultiMC".into(),
        name: cfg
            .get("name")
            .cloned()
            .unwrap_or_else(|| dir.file_name().map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()),
        path: dir.display().to_string(),
        mc_version,
        loader,
        loader_version,
        mod_count,
    })
}

/// Das Spielverzeichnis heisst dort `.minecraft`, seltener `minecraft`.
fn game_dir_of(dir: &Path) -> PathBuf {
    for name in [".minecraft", "minecraft"] {
        let candidate = dir.join(name);
        if candidate.is_dir() {
            return candidate;
        }
    }
    dir.join(".minecraft")
}

/// Übernimmt eine fremde Instanz.
///
/// Kopiert, nicht verschoben -- der andere Launcher bleibt benutzbar.
/// Protokolle und Zwischenspeicher bleiben zurück, die gehören zum alten Lauf.
pub fn import(dir: &Path) -> Result<crate::instance::InstanceConfig> {
    let foreign = inspect(dir)?;
    if foreign.mc_version.is_empty() {
        return Err(Error::other("Die Instanz nennt keine Minecraft-Version"));
    }

    let mut instance =
        crate::instance::create(&foreign.name, &foreign.mc_version, &foreign.loader, false)?;
    instance.fabric_version = foreign.loader_version.clone();
    instance.notes = format!("Übernommen aus {}", foreign.launcher);
    crate::instance::save(&instance)?;

    let from = game_dir_of(dir);
    let to = instance.game_dir()?;
    if from.is_dir() {
        copy_selected(&from, &to)?;
    }

    Ok(instance)
}

const SKIP: [&str; 6] = ["logs", "crash-reports", ".fabric", ".mixin.out", "assets", "versions"];

fn copy_selected(from: &Path, to: &Path) -> Result<()> {
    crate::paths::ensure_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if SKIP.contains(&name.to_string_lossy().as_ref()) {
            continue;
        }
        let target = to.join(&name);
        if entry.path().is_dir() {
            copy_selected(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_wird_gelesen() {
        let raw = "[General]\nname=Meine Instanz\n# Kommentar\nJavaPath=/usr/bin/java\nkaputt\n";
        let cfg = parse_cfg(raw);
        assert_eq!(cfg.get("name").unwrap(), "Meine Instanz");
        assert_eq!(cfg.get("JavaPath").unwrap(), "/usr/bin/java");
        assert!(!cfg.contains_key("kaputt"), "Zeilen ohne = werden übergangen");
        assert_eq!(cfg.len(), 2, "Abschnittsmarke und Kommentar zählen nicht");
    }

    #[test]
    fn bestandteile_ergeben_loader_und_version() {
        let pack: MmcPack = serde_json::from_str(
            r#"{"components":[
                {"uid":"net.minecraft","version":"1.21.4"},
                {"uid":"net.fabricmc.fabric-loader","version":"0.19.5"}
            ]}"#,
        ).unwrap();
        let (loader, lv, mc) = loader_from_components(&pack);
        assert_eq!(loader, "fabric");
        assert_eq!(lv.as_deref(), Some("0.19.5"));
        assert_eq!(mc, "1.21.4");
    }

    #[test]
    fn ohne_loader_bleibt_es_vanilla() {
        let pack: MmcPack = serde_json::from_str(
            r#"{"components":[{"uid":"net.minecraft","version":"1.16.5"}]}"#,
        ).unwrap();
        let (loader, lv, mc) = loader_from_components(&pack);
        assert_eq!(loader, "vanilla");
        assert!(lv.is_none());
        assert_eq!(mc, "1.16.5");
    }

    #[test]
    fn neoforge_wird_erkannt() {
        let pack: MmcPack = serde_json::from_str(
            r#"{"components":[{"uid":"net.neoforged","version":"21.1.80"}]}"#,
        ).unwrap();
        assert_eq!(loader_from_components(&pack).0, "neoforge");
    }

    #[test]
    fn fremder_ordner_wird_abgewiesen() {
        let dir = std::env::temp_dir().join(format!("kein-prism-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(inspect(&dir).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
