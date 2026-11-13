//! Was wir ueber eine installierte Mod wissen.
//!
//! Der Mod-Ordner allein sagt nur, welche Dateien da sind. Woher eine Mod
//! stammt, welche Fassung installiert wurde und wie ihr Symbol aussieht,
//! steht nicht in der Datei -- das wissen wir nur, weil wir sie geholt
//! haben. Deshalb wird es beim Installieren mitgeschrieben.
//!
//! Ohne diese Ablage liesse sich weder auf Updates pruefen noch ein Symbol
//! anzeigen, und beides von der Datei zurueckzuraten geht schief: Namen
//! wie `sodium-fabric-0.8.14+mc1.21.11.jar` sind nicht eindeutig genug.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::paths;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModMeta {
    /// `modrinth` oder `curseforge`.
    pub provider: String,
    pub project_id: String,
    pub version_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    /// Dateinamen der Mods, die diese hier voraussetzt.
    #[serde(default)]
    pub depends_on: Vec<String>,
    /// Wann installiert.
    #[serde(default)]
    pub installed_at: String,
}

/// Zuordnung Dateiname -> Herkunft. Sortiert, damit die Datei bei
/// Aenderungen nicht unnoetig durcheinandergeraet.
pub type MetaMap = BTreeMap<String, ModMeta>;

fn file(instance_id: &str) -> Result<std::path::PathBuf> {
    Ok(paths::instance_dir(instance_id)?.join("mods.json"))
}

pub fn load(instance_id: &str) -> MetaMap {
    file(instance_id)
        .map(|p| crate::store::read_or_default::<MetaMap>(&p))
        .unwrap_or_default()
}

pub fn save(instance_id: &str, map: &MetaMap) -> Result<()> {
    crate::store::write(&file(instance_id)?, map)
}

/// Traegt eine Mod ein.
pub fn set(instance_id: &str, filename: &str, meta: ModMeta) -> Result<()> {
    let mut map = load(instance_id);
    map.insert(filename.to_string(), meta);
    save(instance_id, &map)
}

pub fn remove(instance_id: &str, filename: &str) -> Result<()> {
    let mut map = load(instance_id);
    map.remove(filename);
    // Auch den Gegenpart entfernen, falls die Datei abgeschaltet war.
    map.remove(&toggled_name(filename));
    save(instance_id, &map)
}

/// Verschiebt den Eintrag mit, wenn eine Datei umbenannt wird.
pub fn rename(instance_id: &str, from: &str, to: &str) -> Result<()> {
    let mut map = load(instance_id);
    if let Some(meta) = map.remove(from) {
        map.insert(to.to_string(), meta);
        save(instance_id, &map)?;
    }
    Ok(())
}

/// Der Name derselben Mod im jeweils anderen Zustand.
///
/// Abgeschaltet wird in Minecraft durch Anhaengen von `.disabled` --
/// der Loader ueberliest die Datei dann, sie bleibt aber liegen.
pub fn toggled_name(filename: &str) -> String {
    match filename.strip_suffix(".disabled") {
        Some(base) => base.to_string(),
        None => format!("{filename}.disabled"),
    }
}

pub fn is_enabled(filename: &str) -> bool {
    !filename.ends_with(".disabled")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn umschalten_haengt_die_endung_an_und_wieder_ab() {
        assert_eq!(toggled_name("sodium.jar"), "sodium.jar.disabled");
        assert_eq!(toggled_name("sodium.jar.disabled"), "sodium.jar");
        // Zweimal umschalten fuehrt zurueck.
        assert_eq!(toggled_name(&toggled_name("x.jar")), "x.jar");
    }

    #[test]
    fn zustand_wird_am_namen_erkannt() {
        assert!(is_enabled("sodium.jar"));
        assert!(!is_enabled("sodium.jar.disabled"));
    }
}
