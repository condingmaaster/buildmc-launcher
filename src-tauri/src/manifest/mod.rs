//! Mojangs Versionsmanifest und die Versions-JSON.

pub mod channel;
pub mod rules;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::{http, paths};

pub use channel::{channel_of, VersionChannel};
pub use rules::{Feature, RuleContext};

const MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// Basis fuer Asset-Objekte: `<hash[0..2]>/<hash>`.
pub const ASSET_BASE: &str = "https://resources.download.minecraft.net";

/* ---- Manifest -------------------------------------------------------- */

#[derive(Debug, Clone, Deserialize)]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<ManifestVersion>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub url: String,
    #[serde(rename = "releaseTime")]
    pub release_time: String,
    pub sha1: Option<String>,
}

/// Eine Zeile, wie sie die Oberflaeche sieht: mit abgeleitetem Kanal
/// und dem Hinweis, ob die Version bereits lokal vorliegt.
#[derive(Debug, Clone, Serialize)]
pub struct VersionEntry {
    pub id: String,
    pub kind: String,
    pub channel: VersionChannel,
    pub released_at: String,
    pub installed: bool,
}

pub async fn fetch_manifest() -> Result<VersionManifest> {
    http::get_json(MANIFEST_URL).await
}

/// Das vollstaendige Manifest, um den Kanal ergaenzt.
/// Gefiltert wird in der Oberflaeche, nicht hier.
pub async fn list_versions() -> Result<Vec<VersionEntry>> {
    let manifest = fetch_manifest().await?;
    let dir = paths::versions()?;
    Ok(manifest
        .versions
        .into_iter()
        .map(|v| {
            let installed = dir.join(&v.id).join(format!("{}.json", v.id)).exists();
            VersionEntry {
                channel: channel_of(&v.id, &v.kind),
                id: v.id,
                kind: v.kind,
                released_at: v.release_time,
                installed,
            }
        })
        .collect())
}

/* ---- Versions-JSON --------------------------------------------------- */

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionInfo {
    pub id: String,
    #[serde(rename = "type", default, deserialize_with = "crate::serde_util::null_default")]
    pub kind: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,

    /// Bei Loader-Profilen (Fabric, NeoForge …) die Grundversion,
    /// deren Angaben geerbt und ueberlagert werden.
    #[serde(rename = "inheritsFrom", default)]
    pub inherits_from: Option<String>,

    #[serde(default, deserialize_with = "crate::serde_util::null_default")]
    pub libraries: Vec<Library>,

    #[serde(rename = "assetIndex", default)]
    pub asset_index: Option<AssetIndexRef>,
    #[serde(default)]
    pub assets: Option<String>,

    #[serde(default)]
    pub downloads: Option<VersionDownloads>,

    #[serde(rename = "javaVersion", default)]
    pub java_version: Option<JavaVersionRef>,

    /// Format ab 1.13.
    #[serde(default)]
    pub arguments: Option<Arguments>,
    /// Format vor 1.13: eine einzelne Zeichenkette.
    #[serde(rename = "minecraftArguments", default)]
    pub legacy_arguments: Option<String>,

    #[serde(rename = "logging", default)]
    pub logging: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<serde_json::Value>,
    #[serde(default)]
    pub jvm: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssetIndexRef {
    pub id: String,
    pub url: String,
    pub sha1: String,
    #[serde(default)]
    pub size: u64,
    #[serde(rename = "totalSize", default)]
    pub total_size: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct VersionDownloads {
    pub client: Option<DownloadInfo>,
    pub server: Option<DownloadInfo>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DownloadInfo {
    pub url: String,
    #[serde(default)]
    pub sha1: Option<String>,
    #[serde(default)]
    pub size: u64,
    /// Nur bei Bibliotheken gesetzt.
    #[serde(default)]
    pub path: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct JavaVersionRef {
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
    #[serde(default)]
    pub component: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Library {
    pub name: String,
    #[serde(default)]
    pub downloads: Option<LibraryDownloads>,
    #[serde(default, deserialize_with = "crate::serde_util::null_default")]
    pub rules: Vec<rules::Rule>,
    /// Alte Form: Zuordnung Betriebssystem -> Klassifizierer.
    #[serde(default, deserialize_with = "crate::serde_util::null_default")]
    pub natives: HashMap<String, String>,
    #[serde(default)]
    pub extract: Option<serde_json::Value>,
    /// Fabric und NeoForge liefern nur Koordinaten und eine Maven-Basis.
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct LibraryDownloads {
    #[serde(default)]
    pub artifact: Option<DownloadInfo>,
    #[serde(default, deserialize_with = "crate::serde_util::null_default")]
    pub classifiers: HashMap<String, DownloadInfo>,
}

/* ---- Laden und Vererbung --------------------------------------------- */

/// Liest die Versions-JSON von der Platte.
/// Schliesst aus der Versionsnummer auf die noetige Java-Fassung.
///
/// Die Grenzen folgen Mojangs Umstellungen: 1.17 verlangte erstmals 16,
/// 1.18 dann 17, und ab 1.20.5 sind es 21. Alles davor laeuft mit 8.
/// Versionen ab 25 sind die neue Zaehlweise nach Jahren und brauchen 21.
pub fn java_for_version(id: &str) -> u32 {
    // Loader-Profile heissen `fabric-loader-0.19.5-1.21.4`. Die erste Zahl
    // gehoert dort dem Loader, nicht Minecraft -- gesucht ist der letzte
    // Abschnitt, der wie eine Minecraft-Version aussieht.
    let candidate = id
        .split('-')
        .filter(|part| {
            !part.is_empty()
                && part.chars().next().is_some_and(|c| c.is_ascii_digit())
                && part.chars().all(|c| c.is_ascii_digit() || c == '.')
        })
        .next_back()
        .unwrap_or(id);

    let numbers: Vec<u32> = candidate
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    let (Some(&major), minor, patch) =
        (numbers.first(), numbers.get(1).copied().unwrap_or(0), numbers.get(2).copied().unwrap_or(0))
    else {
        return 8;
    };

    // Neue Zaehlweise ab 25 (Jahresnummer).
    if major >= 25 {
        return 21;
    }
    if major != 1 {
        return 8;
    }
    match minor {
        m if m >= 21 => 21,
        20 if patch >= 5 => 21,
        20 | 19 | 18 => 17,
        17 => 16,
        _ => 8,
    }
}

pub fn read_local(id: &str) -> Result<VersionInfo> {
    let p = paths::version_json(id)?;
    let raw = match std::fs::read(&p) {
        Ok(raw) => raw,
        Err(_) => {
            // Aeltere Installationen (und der ursprungliche 0.1.3-Binary)
            // legten die Profildatei als "profile.json" im Versionsordner
            // ab statt als "<id>.json". Ohne diesen Rueckfall wuerde jede
            // damals installierte Loader-Fassung als "nicht installiert"
            // gelten und der Start faelschlich beim Online-Manifest
            // landen -- wo ein Loader-Profil naturgemaess nie auftaucht.
            let alt = paths::version_dir(id)?.join("profile.json");
            std::fs::read(&alt)
                .map_err(|_| Error::not_found(format!("Version {id} ist nicht installiert")))?
        }
    };
    Ok(serde_json::from_slice(&raw)?)
}

/// Laedt die Versions-JSON aus dem Manifest, falls sie noch fehlt.
pub async fn ensure_version_json(id: &str) -> Result<VersionInfo> {
    let target = paths::version_json(id)?;
    // Die alte Benennung ("profile.json" statt "<id>.json", siehe
    // read_local) muss auch hier erkannt werden -- sonst haelt diese
    // Pruefung eine tatsaechlich vorhandene Loader-Fassung faelschlich
    // fuer fehlend und versucht sie beim Online-Manifest nachzuschlagen,
    // wo Loader-Profile grundsaetzlich nie gelistet sind.
    let alt_vorhanden = paths::version_dir(id).map(|d| d.join("profile.json").exists()).unwrap_or(false);
    if target.exists() || alt_vorhanden {
        return read_local(id);
    }

    let manifest = fetch_manifest().await?;
    let entry = manifest
        .versions
        .iter()
        .find(|v| v.id == id)
        .ok_or_else(|| Error::not_found(format!("Version {id} steht nicht im Manifest")))?;

    let bytes = http::get_bytes(&entry.url).await?;
    paths::ensure_parent(&target)?;
    std::fs::write(&target, &bytes)?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Loest `inheritsFrom` auf und verschmilzt Kind mit Elternteil.
///
/// Fabric und NeoForge liefern nur die Unterschiede zur Grundversion.
/// Die Regel ist: das Kind gewinnt bei Einzelwerten, Listen werden
/// aneinandergehaengt -- Kind zuerst, damit seine Bibliotheken im
/// Klassenpfad vor denen der Grundversion stehen.
pub async fn resolve(id: &str) -> Result<VersionInfo> {
    let mut info = ensure_version_json(id).await?;

    let mut guard = 0;
    while let Some(parent_id) = info.inherits_from.clone() {
        guard += 1;
        if guard > 8 {
            return Err(Error::other("Zu viele Vererbungsebenen in der Versions-JSON"));
        }
        let parent = Box::pin(ensure_version_json(&parent_id)).await?;
        info = merge(info, parent);
    }

    Ok(info)
}

/// Koordinaten ohne Versionsnummer: `group:artifact` plus Klassifizierer.
///
/// Zwei Eintraege mit derselben Kennung sind dieselbe Bibliothek in
/// verschiedenen Fassungen -- beide in den Klassenpfad zu legen, ist der
/// sichere Weg in einen Fehlstart.
fn library_key(name: &str) -> String {
    let parts: Vec<&str> = name.split(':').collect();
    match parts.len() {
        0 | 1 => name.to_string(),
        // group:artifact:version -> group:artifact
        2 | 3 => format!("{}:{}", parts[0], parts[1]),
        // group:artifact:version:classifier -> group:artifact:classifier
        _ => format!("{}:{}:{}", parts[0], parts[1], parts[3]),
    }
}

fn merge(child: VersionInfo, parent: VersionInfo) -> VersionInfo {
    // Der Loader gewinnt: seine Bibliotheken stehen zuerst, und eine
    // gleichnamige aus der Grundversion wird verworfen.
    //
    // Ohne diese Entdoppelung landeten bei Fabric 1.21.4 ASM 9.10.1 und
    // ASM 9.6 gemeinsam im Klassenpfad, und Fabric brach mit
    // "duplicate ASM classes found on classpath" ab.
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut libraries: Vec<Library> = Vec::new();
    for lib in child.libraries.into_iter().chain(parent.libraries) {
        if seen.insert(library_key(&lib.name)) {
            libraries.push(lib);
        }
    }

    let arguments = match (child.arguments, parent.arguments) {
        (Some(c), Some(p)) => Some(Arguments {
            game: [p.game, c.game].concat(),
            jvm: [p.jvm, c.jvm].concat(),
        }),
        (Some(c), None) => Some(c),
        (None, p) => p,
    };

    VersionInfo {
        id: child.id,
        kind: if child.kind.is_empty() { parent.kind } else { child.kind },
        main_class: child.main_class,
        inherits_from: parent.inherits_from,
        libraries,
        asset_index: child.asset_index.or(parent.asset_index),
        assets: child.assets.or(parent.assets),
        downloads: child.downloads.or(parent.downloads),
        java_version: child.java_version.or(parent.java_version),
        arguments,
        legacy_arguments: child.legacy_arguments.or(parent.legacy_arguments),
        logging: child.logging.or(parent.logging),
    }
}

impl VersionInfo {
    /// Benoetigte Java-Hauptversion.
    ///
    /// Steht sie im Manifest, gilt sie. Fehlt sie, wird aus der
    /// Versionsnummer geschlossen -- und das ist kein Sonderfall: eine
    /// lokal abgelegte Versions-JSON kann beschnitten sein. Genau das
    /// wurde in einer Installation von 0.1.3 gefunden, wo bei 1.21.4 das
    /// Feld fehlte. Blind 8 anzunehmen haette dort ein Java gewaehlt, mit
    /// dem das Spiel nicht startet.
    pub fn required_java(&self) -> u32 {
        if let Some(j) = &self.java_version {
            return j.major_version;
        }
        java_for_version(&self.id)
    }

    pub fn asset_index_id(&self) -> String {
        self.asset_index
            .as_ref()
            .map(|a| a.id.clone())
            .or_else(|| self.assets.clone())
            .unwrap_or_else(|| "legacy".into())
    }
}

#[cfg(test)]
mod version_tests {
    use super::java_for_version;

    #[test]
    fn java_bedarf_aus_der_versionsnummer() {
        // Der Anlass: in einer echten 0.1.3-Installation fehlte bei 1.21.4
        // das Feld javaVersion. Blind 8 anzunehmen haette ein Java gewaehlt,
        // mit dem das Spiel nicht startet.
        assert_eq!(java_for_version("1.21.4"), 21);
        assert_eq!(java_for_version("1.20.5"), 21);
        assert_eq!(java_for_version("1.20.4"), 17);
        assert_eq!(java_for_version("1.18"), 17);
        assert_eq!(java_for_version("1.17.1"), 16);
        assert_eq!(java_for_version("1.16.5"), 8);
        assert_eq!(java_for_version("1.8.9"), 8);
    }

    #[test]
    fn neue_zaehlweise_nach_jahren() {
        assert_eq!(java_for_version("26.1.2"), 21);
        assert_eq!(java_for_version("26.2"), 21);
        assert_eq!(java_for_version("25.1"), 21);
    }

    /// Aus dem echten Fabric-Start: das Profil heisst
    /// `fabric-loader-0.19.5-1.21.4`, und die fuehrende 0 des Loaders
    /// fuehrte zu Java 8 statt 21.
    #[test]
    fn loader_profile_nennen_die_minecraft_version() {
        assert_eq!(java_for_version("fabric-loader-0.19.5-1.21.4"), 21);
        assert_eq!(java_for_version("quilt-loader-0.29.1-1.21.4"), 21);
        assert_eq!(java_for_version("fabric-loader-0.19.5-1.16.5"), 8);
        assert_eq!(java_for_version("fabric-loader-0.14.9-1.18.2"), 17);
    }

    #[test]
    fn vorabfassungen_werden_richtig_gelesen() {
        assert_eq!(java_for_version("1.21.4-pre1"), 21);
        assert_eq!(java_for_version("1.16.5-rc1"), 8);
    }

    #[test]
    fn unbekanntes_faellt_auf_acht_zurueck() {
        assert_eq!(java_for_version(""), 8);
        assert_eq!(java_for_version("kaputt"), 8);
        // Alte Beta-Kennungen tragen keine brauchbare Nummer.
        assert_eq!(java_for_version("b1.7.3"), 8);
    }
}


#[cfg(test)]
mod merge_tests {
    use super::*;

    fn lib(name: &str) -> Library {
        Library {
            name: name.into(), downloads: None, rules: vec![],
            natives: Default::default(), extract: None, url: None,
        }
    }

    fn info(id: &str, libs: Vec<Library>) -> VersionInfo {
        VersionInfo {
            id: id.into(), kind: "release".into(), main_class: "M".into(),
            inherits_from: None, libraries: libs, asset_index: None,
            assets: None, downloads: None, java_version: None,
            arguments: None, legacy_arguments: None, logging: None,
        }
    }

    #[test]
    fn koordinaten_ohne_fassung() {
        assert_eq!(library_key("org.ow2.asm:asm:9.6"), "org.ow2.asm:asm");
        assert_eq!(library_key("org.ow2.asm:asm:9.10.1"), "org.ow2.asm:asm");
        // Der Klassifizierer unterscheidet sehr wohl.
        assert_eq!(
            library_key("org.lwjgl:lwjgl:3.3.3:natives-linux"),
            "org.lwjgl:lwjgl:natives-linux"
        );
    }

    /// Der Fehler aus dem echten Fabric-Start:
    /// "duplicate ASM classes found on classpath".
    #[test]
    fn doppelte_bibliothek_faellt_weg_und_der_loader_gewinnt() {
        let child = info("fabric", vec![lib("org.ow2.asm:asm:9.10.1"), lib("net.fabricmc:loader:1")]);
        let parent = info("1.21.4", vec![lib("org.ow2.asm:asm:9.6"), lib("com.mojang:authlib:6")]);

        let merged = merge(child, parent);
        let namen: Vec<&str> = merged.libraries.iter().map(|l| l.name.as_str()).collect();

        assert_eq!(namen.len(), 3, "ASM darf nur einmal vorkommen");
        assert!(namen.contains(&"org.ow2.asm:asm:9.10.1"), "die Fassung des Loaders gewinnt");
        assert!(!namen.contains(&"org.ow2.asm:asm:9.6"));
        assert!(namen.contains(&"com.mojang:authlib:6"), "Übriges der Grundversion bleibt");
    }

    #[test]
    fn natives_bleiben_neben_der_gewoehnlichen_bibliothek() {
        let child = info("a", vec![lib("org.lwjgl:lwjgl:3.3.3")]);
        let parent = info("b", vec![lib("org.lwjgl:lwjgl:3.3.3:natives-linux")]);
        assert_eq!(merge(child, parent).libraries.len(), 2,
                   "Klassifizierer machen daraus zwei verschiedene Einträge");
    }
}
