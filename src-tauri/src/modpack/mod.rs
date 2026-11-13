//! Modpacks im `.mrpack`-Format.
//!
//! Ein Modpack ist ein Zip mit zwei Teilen:
//!
//! * `modrinth.index.json` -- nennt Minecraft-Version, Loader und jede Mod
//!   mit Adresse und Pruefsumme. Die Dateien selbst liegen **nicht** im
//!   Archiv; sie werden beim Einlesen geholt. Das haelt das Paket klein
//!   und umgeht die Frage, ob man fremde Mods weiterverteilen darf.
//! * `overrides/` -- alles, was zusaetzlich ins Spielverzeichnis gehoert:
//!   Konfigurationen, Resourcepacks, gelegentlich Welten.
//!
//! Beim Ausgeben wird dieselbe Trennung eingehalten: Mods aus den Portalen
//! werden als Verweis eingetragen, nur eigene Dateien wandern ins Archiv.

pub mod foreign;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

const INDEX_NAME: &str = "modrinth.index.json";

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PackIndex {
    #[serde(rename = "formatVersion", default = "one")]
    pub format_version: u32,
    #[serde(default = "modrinth")]
    pub game: String,
    #[serde(rename = "versionId", default)]
    pub version_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub files: Vec<PackFile>,
    /// `minecraft`, `fabric-loader`, `quilt-loader`, `neoforge`, `forge`.
    #[serde(default)]
    pub dependencies: HashMap<String, String>,
}

fn one() -> u32 { 1 }
fn modrinth() -> String { "minecraft".into() }

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PackFile {
    /// Zielpfad im Spielverzeichnis, z. B. `mods/sodium.jar`.
    pub path: String,
    #[serde(default)]
    pub hashes: HashMap<String, String>,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize", default)]
    pub file_size: u64,
    /// `client`/`server` je `required`, `optional`, `unsupported`.
    #[serde(default)]
    pub env: Option<HashMap<String, String>>,
}

impl PackFile {
    /// Braucht der Client diese Datei?
    ///
    /// Viele Pakete fuehren serverseitige Mods mit. Sie mitzuladen kostet
    /// Zeit und kann den Client sogar stoeren.
    pub fn needed_on_client(&self) -> bool {
        match &self.env {
            None => true,
            Some(env) => env.get("client").map(|v| v != "unsupported").unwrap_or(true),
        }
    }

    /// Verhindert, dass ein Paket ausserhalb des Spielverzeichnisses schreibt.
    ///
    /// Ein Pfad wie `../../.bashrc` waere sonst eine offene Tuer. Das ist
    /// kein erdachtes Risiko -- Modpacks kommen von Fremden.
    pub fn safe_path(&self) -> Option<std::path::PathBuf> {
        let p = Path::new(&self.path);
        if p.is_absolute() {
            return None;
        }
        let mut out = std::path::PathBuf::new();
        for part in p.components() {
            match part {
                std::path::Component::Normal(x) => out.push(x),
                // Alles andere -- `..`, Laufwerksbuchstaben, Wurzel -- ist unzulaessig.
                _ => return None,
            }
        }
        (!out.as_os_str().is_empty()).then_some(out)
    }
}

/// Was vor dem Einlesen angezeigt wird.
#[derive(Debug, Clone, Serialize)]
pub struct PackPreview {
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub mod_count: usize,
    pub total_bytes: u64,
    /// Dateien, die wegen eines unzulaessigen Pfades ausgelassen werden.
    pub rejected: Vec<String>,
}

/// Liest den Index eines Pakets, ohne etwas zu schreiben.
pub fn preview(path: &Path) -> Result<PackPreview> {
    let index = read_index(path)?;
    let (loader, loader_version) = loader_of(&index);

    let files: Vec<&PackFile> = index.files.iter().filter(|f| f.needed_on_client()).collect();
    let rejected: Vec<String> = files
        .iter()
        .filter(|f| f.safe_path().is_none())
        .map(|f| f.path.clone())
        .collect();

    Ok(PackPreview {
        name: if index.name.is_empty() { "Ohne Namen".into() } else { index.name.clone() },
        version: index.version_id.clone(),
        summary: index.summary.clone(),
        mc_version: index.dependencies.get("minecraft").cloned().unwrap_or_default(),
        loader,
        loader_version,
        mod_count: files.len(),
        total_bytes: files.iter().map(|f| f.file_size).sum(),
        rejected,
    })
}

pub fn read_index(path: &Path) -> Result<PackIndex> {
    let file = std::fs::File::open(path)
        .map_err(|_| Error::not_found(format!("{} lässt sich nicht öffnen", path.display())))?;
    let mut zip = zip::ZipArchive::new(file)?;

    let mut entry = zip
        .by_name(INDEX_NAME)
        .map_err(|_| Error::other("Das ist kein Modrinth-Modpack — die Indexdatei fehlt."))?;

    let mut raw = String::new();
    entry.read_to_string(&mut raw)?;
    Ok(serde_json::from_str(&raw)?)
}

/// Bestimmt Loader und Fassung aus den Abhaengigkeiten.
pub fn loader_of(index: &PackIndex) -> (String, Option<String>) {
    for (key, loader) in [
        ("fabric-loader", "fabric"),
        ("quilt-loader", "quilt"),
        ("neoforge", "neoforge"),
        ("forge", "forge"),
    ] {
        if let Some(version) = index.dependencies.get(key) {
            return (loader.to_string(), Some(version.clone()));
        }
    }
    ("vanilla".to_string(), None)
}

/// Legt eine Instanz aus einem Paket an.
///
/// Gibt die angelegte Instanz und die noch zu ladenden Dateien zurueck --
/// das Herunterladen uebernimmt der Aufrufer, damit der Fortschritt
/// sichtbar wird.
pub fn import(path: &Path) -> Result<(crate::instance::InstanceConfig, Vec<crate::download::Item>)> {
    let index = read_index(path)?;
    let (loader, loader_version) = loader_of(&index);
    let mc_version = index
        .dependencies
        .get("minecraft")
        .cloned()
        .ok_or_else(|| Error::other("Das Paket nennt keine Minecraft-Version"))?;

    let name = if index.name.is_empty() { "Modpack".to_string() } else { index.name.clone() };
    let mut instance = crate::instance::create(&name, &mc_version, &loader, false)?;
    instance.fabric_version = loader_version;
    instance.notes = index.summary.clone().unwrap_or_default();
    crate::instance::save(&instance)?;

    let game_dir = instance.game_dir()?;
    let mut items = Vec::new();

    for file in index.files.iter().filter(|f| f.needed_on_client()) {
        let Some(relative) = file.safe_path() else {
            eprintln!("[modpack] Pfad abgelehnt: {}", file.path);
            continue;
        };
        let Some(url) = file.downloads.first() else { continue };

        items.push(crate::download::Item {
            url: url.clone(),
            target: game_dir.join(relative),
            sha1: file.hashes.get("sha1").cloned(),
            size: file.file_size,
        });
    }

    extract_overrides(path, &game_dir)?;
    Ok((instance, items))
}

/// Entpackt `overrides/` und `client-overrides/` ins Spielverzeichnis.
fn extract_overrides(archive: &Path, game_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
        let as_str = name.to_string_lossy().replace('\\', "/");

        let relative = as_str
            .strip_prefix("overrides/")
            .or_else(|| as_str.strip_prefix("client-overrides/"));
        let Some(relative) = relative.filter(|r| !r.is_empty()) else { continue };

        let probe = PackFile {
            path: relative.to_string(),
            hashes: HashMap::new(),
            downloads: vec![],
            file_size: 0,
            env: None,
        };
        let Some(safe) = probe.safe_path() else { continue };
        let target = game_dir.join(safe);

        if entry.is_dir() {
            paths::ensure_dir(&target)?;
            continue;
        }
        paths::ensure_parent(&target)?;
        let mut sink = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut sink)?;
    }
    Ok(())
}

/* ---- Ausgeben ------------------------------------------------------------ */

#[derive(Debug, Clone, Deserialize)]
pub struct ExportOptions {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub summary: Option<String>,
    /// Konfigurationen mitnehmen.
    #[serde(default = "yes")]
    pub include_config: bool,
    /// Resourcepacks und Shader mitnehmen.
    #[serde(default = "yes")]
    pub include_packs: bool,
    /// Welten mitnehmen. Voreinstellung nein -- sie sind gross und
    /// gehoeren selten in ein Paket zur Weitergabe.
    #[serde(default)]
    pub include_worlds: bool,
}

fn yes() -> bool { true }

/// Schreibt ein `.mrpack`, das sich in der Modrinth App (und anderen Launchern) 1:1 importieren lässt.
///
/// Mods mit Modrinth-Quelle werden als Verweis mit echter Download-Adresse und beiden Prüfsummen
/// (sha1, sha512) eingetragen, sofern die lokale Datei genau der Datei von Modrinth entspricht.
/// Alle anderen Mods (CurseForge, von Hand hinzugefügt, verändert) werden als Datei unter
/// `overrides/mods/` mitgepackt, damit nichts fehlt. Dazu kommen Einstellungen, Server-Liste und
/// auf Wunsch Konfiguration, Ressourcen-/Shaderpakete und Welten. Rückgabe: Mods, die mitgepackt wurden.
pub async fn export(
    instance_id: &str,
    options: &ExportOptions,
    output: &Path,
) -> Result<Vec<String>> {
    use sha1::{Digest, Sha1};
    use sha2::Sha512;

    let instance = crate::instance::load(instance_id)?;
    let game_dir = instance.game_dir()?;
    let known = crate::mods::meta::load(instance_id);

    let mut dependencies = HashMap::new();
    dependencies.insert("minecraft".to_string(), instance.mc_version.clone());
    let loader_key = match instance.loader.as_str() {
        "quilt" => Some(("quilt-loader", crate::loader::LoaderKind::Quilt)),
        "neoforge" | "forge" => None,
        _ => Some(("fabric-loader", crate::loader::LoaderKind::Fabric)),
    };
    match (&instance.fabric_version, loader_key) {
        (Some(v), Some((key, _))) if !v.is_empty() => { dependencies.insert(key.to_string(), v.clone()); }
        (_, Some((key, kind))) => {
            if let Ok(v) = crate::loader::recommended(kind, &instance.mc_version).await { dependencies.insert(key.to_string(), v); }
        }
        (Some(v), None) => { dependencies.insert(instance.loader.clone(), v.clone()); }
        _ => {}
    }

    // Erst alle Mods einlesen und hashen, dann die Modrinth-Abfragen gleichzeitig (viele kleine Anfragen nacheinander dauern lange).
    struct Local { filename: String, path: std::path::PathBuf, size: u64, sha1: String, sha512: String, version_id: Option<String> }
    let mut locals = Vec::new();
    for m in crate::mods::list(instance_id)? {
        if !m.enabled {
            continue;
        }
        let path = game_dir.join("mods").join(&m.filename);
        let bytes = std::fs::read(&path)?;
        let version_id = known
            .get(&m.filename)
            .filter(|x| x.provider == "modrinth" && !x.version_id.is_empty())
            .map(|x| x.version_id.clone());
        locals.push(Local {
            filename: m.filename.clone(), path, size: bytes.len() as u64,
            sha1: hex::encode(Sha1::digest(&bytes)), sha512: hex::encode(Sha512::digest(&bytes)), version_id,
        });
    }
    let lookups = locals.iter().map(|l| {
        let url = l.version_id.as_ref().map(|v| format!("https://api.modrinth.com/v2/version/{v}"));
        async move {
            match url {
                Some(u) => crate::http::get_json::<serde_json::Value>(&u).await.ok(),
                None => None,
            }
        }
    });
    let answers = futures::future::join_all(lookups).await;

    let mut files = Vec::new();
    let mut bundled: Vec<(String, std::path::PathBuf)> = Vec::new();
    for (l, answer) in locals.into_iter().zip(answers) {
        let reference = answer.and_then(|v| {
            v["files"].as_array()?.iter()
                .find(|f| f["hashes"]["sha1"].as_str() == Some(l.sha1.as_str()))?["url"].as_str().map(str::to_string)
        });
        match reference {
            Some(url) => files.push(PackFile {
                path: format!("mods/{}", l.filename),
                hashes: HashMap::from([("sha1".to_string(), l.sha1), ("sha512".to_string(), l.sha512)]),
                downloads: vec![url],
                file_size: l.size,
                env: Some(HashMap::from([("client".to_string(), "required".to_string()), ("server".to_string(), "required".to_string())])),
            }),
            None => bundled.push((l.filename, l.path)),
        }
    }

    let index = PackIndex {
        format_version: 1,
        game: "minecraft".into(),
        version_id: if options.version.is_empty() { "1.0.0".into() } else { options.version.clone() },
        name: if options.name.is_empty() { instance.name.clone() } else { options.name.clone() },
        summary: options.summary.clone(),
        files,
        dependencies,
    };

    paths::ensure_parent(output)?;
    let file = std::fs::File::create(output)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    zip.start_file(INDEX_NAME, opts)?;
    zip.write_all(&serde_json::to_vec_pretty(&index)?)?;

    // Mods, die sich nicht verweisen lassen, reisen als Datei mit.
    for (filename, local) in &bundled {
        zip.start_file(format!("overrides/mods/{filename}"), opts)?;
        std::io::copy(&mut std::fs::File::open(local)?, &mut zip)?;
    }
    // Einstellungen und Server-Liste gehören zu einer 1:1-Kopie.
    for name in ["options.txt", "servers.dat"] {
        let path = game_dir.join(name);
        if path.is_file() {
            zip.start_file(format!("overrides/{name}"), opts)?;
            std::io::copy(&mut std::fs::File::open(&path)?, &mut zip)?;
        }
    }

    let mut folders: Vec<&str> = Vec::new();
    if options.include_config { folders.push("config"); }
    if options.include_packs { folders.push("resourcepacks"); folders.push("shaderpacks"); }
    if options.include_worlds { folders.push("saves"); }

    for folder in folders {
        let dir = game_dir.join(folder);
        if dir.exists() {
            add_override(&mut zip, &game_dir, &dir, &opts)?;
        }
    }

    zip.finish()?;
    Ok(bundled.into_iter().map(|(name, _)| name).collect())
}

fn add_override(
    zip: &mut zip::ZipWriter<std::fs::File>,
    game_dir: &Path,
    current: &Path,
    opts: &zip::write::FileOptions<'_, ()>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let relative = path.strip_prefix(game_dir).unwrap_or(&path);
        let name = format!("overrides/{}", relative.to_string_lossy().replace('\\', "/"));

        // Die Verwaltungsdateien des Launchers gehoeren nicht ins Paket.
        let name_only = path.file_name().map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if matches!(name_only.as_str(), "instance.json" | "mods.json") {
            continue;
        }

        if path.is_dir() {
            add_override(zip, game_dir, &path, opts)?;
        } else {
            zip.start_file(name, *opts)?;
            let mut source = std::fs::File::open(&path)?;
            std::io::copy(&mut source, zip)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod export_real {
    /// Von Hand: `BUILDMC_EXPORT_TEST=<instanz-id> cargo test export_real -- --ignored --nocapture`
    #[tokio::test]
    #[ignore]
    async fn export_real_instance() {
        let Ok(id) = std::env::var("BUILDMC_EXPORT_TEST") else { return };
        let out = std::path::PathBuf::from("/tmp/buildmc-export-test.mrpack");
        let opts = super::ExportOptions { name: String::new(), version: "1.0.0".into(), summary: None, include_config: true, include_packs: true, include_worlds: false };
        let bundled = super::export(&id, &opts, &out).await.unwrap();
        println!("bundled: {bundled:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_with(path: &str, env: Option<&[(&str, &str)]>) -> PackFile {
        PackFile {
            path: path.into(),
            hashes: HashMap::new(),
            downloads: vec!["https://example.org/x".into()],
            file_size: 10,
            env: env.map(|pairs| {
                pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
            }),
        }
    }

    #[test]
    fn ausbruch_aus_dem_spielverzeichnis_wird_verhindert() {
        // Modpacks kommen von Fremden -- das ist kein erdachtes Risiko.
        assert!(file_with("../../.bashrc", None).safe_path().is_none());
        assert!(file_with("/etc/passwd", None).safe_path().is_none());
        assert!(file_with("mods/../../x.jar", None).safe_path().is_none());
        // Gewoehnliche Pfade bleiben zulaessig.
        assert_eq!(
            file_with("mods/sodium.jar", None).safe_path(),
            Some(std::path::PathBuf::from("mods/sodium.jar"))
        );
    }

    #[test]
    fn serverseitige_mods_werden_ausgelassen() {
        assert!(!file_with("mods/s.jar", Some(&[("client", "unsupported")])).needed_on_client());
        assert!(file_with("mods/c.jar", Some(&[("client", "required")])).needed_on_client());
        assert!(file_with("mods/o.jar", Some(&[("client", "optional")])).needed_on_client());
        // Ohne Angabe gilt: wird gebraucht.
        assert!(file_with("mods/x.jar", None).needed_on_client());
    }

    #[test]
    fn loader_wird_aus_den_abhaengigkeiten_gelesen() {
        let mut deps = HashMap::new();
        deps.insert("minecraft".to_string(), "1.21.4".to_string());
        deps.insert("fabric-loader".to_string(), "0.19.5".to_string());

        let index = PackIndex {
            format_version: 1, game: "minecraft".into(), version_id: "1".into(),
            name: "P".into(), summary: None, files: vec![], dependencies: deps,
        };
        assert_eq!(loader_of(&index), ("fabric".into(), Some("0.19.5".into())));
    }

    #[test]
    fn ohne_loader_ist_es_vanilla() {
        let mut deps = HashMap::new();
        deps.insert("minecraft".to_string(), "1.21.4".to_string());
        let index = PackIndex {
            format_version: 1, game: "minecraft".into(), version_id: "1".into(),
            name: "P".into(), summary: None, files: vec![], dependencies: deps,
        };
        assert_eq!(loader_of(&index), ("vanilla".into(), None));
    }

    #[test]
    fn index_ohne_pflichtfelder_bleibt_lesbar() {
        let index: PackIndex = serde_json::from_str(r#"{"dependencies":{"minecraft":"1.20.1"}}"#)
            .expect("muss lesbar sein");
        assert_eq!(index.format_version, 1);
        assert!(index.files.is_empty());
    }
}
