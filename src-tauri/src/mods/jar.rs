//! Was in einer Mod-Datei steht.
//!
//! Wird eine Mod ueber den Launcher installiert, kennen wir Titel, Fassung
//! und Symbol aus dem Portal. Legt der Nutzer aber eine Datei von Hand in
//! den Ordner -- und das tun viele --, gibt es nichts davon. Dann bleibt
//! nur, in die Datei selbst zu sehen.
//!
//! Fabric und Quilt legen eine JSON-Datei in die Wurzel des Archivs,
//! Forge und NeoForge eine TOML-Datei unter `META-INF`. Gelesen wird
//! beides; scheitert es, bleibt der Dateiname als Notbehelf.

use std::io::Read;
use std::path::Path;

use serde::Deserialize;

/// Die Angaben, die eine Mod ueber sich macht.
#[derive(Debug, Clone, PartialEq)]
pub struct JarInfo {
    pub mod_id: String,
    pub name: String,
    pub version: String,
    /// Woher die Angaben stammen -- `fabric`, `forge` oder `dateiname`.
    pub origin: &'static str,
}

#[derive(Debug, Deserialize)]
struct FabricModJson {
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    version: Option<String>,
}

/// Liest die Angaben aus einer Mod-Datei.
pub fn read(path: &Path) -> JarInfo {
    read_archive(path).unwrap_or_else(|| from_filename(path))
}

fn read_archive(path: &Path) -> Option<JarInfo> {
    let file = std::fs::File::open(path).ok()?;
    let mut zip = zip::ZipArchive::new(file).ok()?;

    // Fabric und Quilt zuerst -- die verbreitetste Form.
    for name in ["fabric.mod.json", "quilt.mod.json"] {
        if let Some(info) = read_fabric(&mut zip, name) {
            return Some(info);
        }
    }

    for name in ["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Some(info) = read_toml(&mut zip, name) {
            return Some(info);
        }
    }

    None
}

fn read_fabric<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    entry: &str,
) -> Option<JarInfo> {
    let mut file = zip.by_name(entry).ok()?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).ok()?;

    let parsed: FabricModJson = serde_json::from_str(&raw).ok()?;
    Some(JarInfo {
        name: parsed.name.clone().unwrap_or_else(|| parsed.id.clone()),
        version: parsed.version.unwrap_or_else(|| "?".into()),
        mod_id: parsed.id,
        origin: "fabric",
    })
}

/// Liest die drei Felder, die uns interessieren, ohne TOML-Bibliothek.
///
/// Die Dateien sind flach genug, dass ein zeilenweises Lesen genuegt --
/// und eine weitere Abhaengigkeit fuer drei Werte waere unverhaeltnismaessig.
fn read_toml<R: Read + std::io::Seek>(
    zip: &mut zip::ZipArchive<R>,
    entry: &str,
) -> Option<JarInfo> {
    let mut file = zip.by_name(entry).ok()?;
    let mut raw = String::new();
    file.read_to_string(&mut raw).ok()?;
    parse_toml_fields(&raw)
}

fn parse_toml_fields(raw: &str) -> Option<JarInfo> {
    let mut mod_id = None;
    let mut name = None;
    let mut version = None;

    for line in raw.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let value = value.trim().trim_matches('"').trim_matches('\'').to_string();
        if value.is_empty() {
            continue;
        }
        match key.trim() {
            "modId" if mod_id.is_none() => mod_id = Some(value),
            "displayName" if name.is_none() => name = Some(value),
            "version" if version.is_none() => version = Some(value),
            _ => {}
        }
    }

    let mod_id = mod_id?;
    Some(JarInfo {
        name: name.unwrap_or_else(|| mod_id.clone()),
        // Forge schreibt haeufig den Platzhalter ${file.jarVersion}.
        version: version
            .filter(|v| !v.contains("${"))
            .unwrap_or_else(|| "?".into()),
        mod_id,
        origin: "forge",
    })
}

/// Notbehelf: aus `sodium-fabric-0.8.14+mc1.21.11.jar` einen lesbaren
/// Namen und eine Fassung gewinnen.
pub fn from_filename(path: &Path) -> JarInfo {
    let stem = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let stem = stem
        .trim_end_matches(".disabled")
        .trim_end_matches(".jar")
        .to_string();

    // Der erste Abschnitt, der mit einer Ziffer beginnt, leitet die Fassung ein.
    let parts: Vec<&str> = stem.split('-').collect();
    let cut = parts
        .iter()
        .position(|p| p.chars().next().is_some_and(|c| c.is_ascii_digit()));

    let (name_parts, version_parts) = match cut {
        Some(i) => (&parts[..i], &parts[i..]),
        None => (&parts[..], &[][..]),
    };

    let name = if name_parts.is_empty() { stem.clone() } else { name_parts.join(" ") };

    JarInfo {
        mod_id: name.to_lowercase().replace(' ', "-"),
        name,
        version: if version_parts.is_empty() { "?".into() } else { version_parts.join("-") },
        origin: "dateiname",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn name_und_fassung_aus_dem_dateinamen() {
        let i = from_filename(Path::new("sodium-fabric-0.8.14+mc1.21.11.jar"));
        assert_eq!(i.name, "sodium fabric");
        assert_eq!(i.version, "0.8.14+mc1.21.11");
        assert_eq!(i.origin, "dateiname");
    }

    #[test]
    fn abgeschaltete_datei_wird_genauso_gelesen() {
        let i = from_filename(Path::new("lithium-fabric-0.21.4.jar.disabled"));
        assert_eq!(i.name, "lithium fabric");
        assert_eq!(i.version, "0.21.4");
    }

    #[test]
    fn dateiname_ohne_fassung() {
        let i = from_filename(Path::new("WitherHearts.jar"));
        assert_eq!(i.name, "WitherHearts");
        assert_eq!(i.version, "?");
    }

    #[test]
    fn toml_felder_werden_gelesen() {
        let raw = r#"
            modLoader = "javafml"
            [[mods]]
            modId = "jei"
            displayName = "Just Enough Items"
            version = "19.5.0"
        "#;
        let i = parse_toml_fields(raw).unwrap();
        assert_eq!(i.mod_id, "jei");
        assert_eq!(i.name, "Just Enough Items");
        assert_eq!(i.version, "19.5.0");
    }

    #[test]
    fn toml_platzhalter_gilt_nicht_als_fassung() {
        let raw = "modId = \"x\"\nversion = \"${file.jarVersion}\"";
        assert_eq!(parse_toml_fields(raw).unwrap().version, "?");
    }

    #[test]
    fn toml_ohne_modid_ergibt_nichts() {
        assert!(parse_toml_fields("displayName = \"nur ein Name\"").is_none());
    }
}
