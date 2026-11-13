//! Unsere Anbindung an CurseForge.
//!
//! Zweiter Anbieter neben Modrinth -- ergaenzend, nicht ersetzend: manche
//! Mods gibt es nur hier. Liefert dieselben oeffentlichen Typen wie das
//! Modrinth-Modul (`crate::modrinth::{SearchResult, ModVersion, SearchPage}`),
//! damit alles Weitere -- Suche zusammenfuehren, Anzeige, Installation --
//! keinen Unterschied zwischen den Anbietern kennen muss.
//!
//! Ohne Schluessel bleibt dieser Anbieter schlicht leer, nie ein Fehler --
//! CurseForge ist eine Ergaenzung, kein Ersatz fuer Modrinth.
//!
//! Numerische Werte sind gegen die offizielle Dokumentation
//! (docs.curseforge.com) und zwei unabhaengige Fremdbibliotheken geprueft,
//! nicht angenommen:
//!
//! * `gameId` fuer Minecraft: `432`
//! * `classId` fuer Mods: `6` (Modpacks/Resourcepacks/Welten sind *nicht*
//!   verifiziert und werden deshalb bewusst nicht gefiltert, siehe unten)
//! * `modLoaderType`: 0 Beliebig, 1 Forge, 2 Cauldron, 3 LiteLoader,
//!   4 Fabric, 5 Quilt, 6 NeoForge
//! * Hash-Algorithmus: 1 SHA1, 2 MD5

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::http;
use crate::modrinth::{ModDependency, ModVersion, SearchPage, SearchResult};

/// Was die Oberflaeche ueber den Zustand dieses Anbieters wissen muss.
#[derive(Debug, Clone, Serialize)]
pub struct Status {
    pub configured: bool,
    pub key_hint: Option<String>,
    pub from_env: bool,
}

pub fn status() -> Status {
    Status {
        configured: crate::ai::keys::curseforge_api_key().is_some(),
        key_hint: crate::ai::keys::curseforge_masked(),
        from_env: std::env::var(crate::ai::keys::CURSEFORGE_ENV_VAR)
            .is_ok_and(|v| !v.trim().is_empty()),
    }
}

const BASE: &str = "https://api.curseforge.com/v1";
const HEADER: &str = "x-api-key";

const MINECRAFT_GAME_ID: u32 = 432;
/// Einzige verifizierte Klasse. Andere Inhaltstypen (Modpacks,
/// Resourcepacks, Welten) haben eigene `classId`-Werte, die sich in der
/// oeffentlichen Dokumentation nicht zweifelsfrei bestaetigen liessen --
/// lieber CurseForge fuer diese Typen aussparen als mit einem falschen
/// Wert falsche Treffer liefern.
const MOD_CLASS_ID: u32 = 6;

/// Loader-Namen wie wir sie intern fuehren, auf CurseForges Zahlen
/// abgebildet. BuildMC verwendet Fabric, Vanilla bekommt keinen Loaderfilter.
fn loader_type(loader: &str) -> Option<u8> {
    match crate::modrinth::mod_loader(loader) {
        "forge" => Some(1),
        "fabric" => Some(4),
        "quilt" => Some(5),
        "neoforge" => Some(6),
        _ => None,
    }
}

/* ---- Rohformat von CurseForge ------------------------------------------- */

#[derive(Debug, Clone, Deserialize)]
struct SearchResponse {
    data: Vec<RawMod>,
    pagination: Pagination,
}

#[derive(Debug, Clone, Deserialize)]
struct Pagination {
    #[serde(rename = "totalCount")]
    total_count: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct RawMod {
    id: u64,
    name: String,
    slug: String,
    summary: String,
    #[serde(rename = "downloadCount")]
    download_count: u64,
    #[serde(rename = "logo")]
    logo: Option<RawLogo>,
    categories: Vec<RawCategory>,
    authors: Vec<RawAuthor>,
    #[serde(rename = "latestFilesIndexes", default)]
    latest_files_indexes: Vec<RawFileIndex>,
    #[serde(rename = "dateModified")]
    date_modified: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawLogo {
    #[serde(rename = "thumbnailUrl")]
    thumbnail_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawCategory {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawAuthor {
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFileIndex {
    #[serde(rename = "gameVersion")]
    game_version: String,
    #[serde(rename = "modLoader")]
    mod_loader: Option<u8>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModResponse {
    data: RawMod,
}

#[derive(Debug, Clone, Deserialize)]
struct FilesResponse {
    data: Vec<RawFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFile {
    id: u64,
    #[serde(rename = "modId")]
    mod_id: u64,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "fileName")]
    file_name: String,
    #[serde(rename = "fileDate")]
    file_date: String,
    #[serde(rename = "downloadCount")]
    download_count: u64,
    #[serde(rename = "fileLength")]
    file_length: u64,
    #[serde(rename = "releaseType")]
    release_type: u8,
    #[serde(rename = "downloadUrl")]
    download_url: Option<String>,
    #[serde(rename = "gameVersions", default)]
    game_versions: Vec<String>,
    #[serde(default)]
    hashes: Vec<RawHash>,
    #[serde(default)]
    dependencies: Vec<RawFileDependency>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawHash {
    value: String,
    algo: u8,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFileDependency {
    #[serde(rename = "modId")]
    mod_id: u64,
    #[serde(rename = "relationType")]
    relation_type: u8,
}

/* ---- Umsetzung in unsere Typen ------------------------------------------ */

fn release_type_name(t: u8) -> &'static str {
    // 1 Release, 2 Beta, 3 Alpha -- ebenfalls in der Dokumentation belegt.
    match t {
        1 => "release",
        2 => "beta",
        _ => "alpha",
    }
}

/// 1 EmbeddedLibrary, 2 OptionalDependency, 3 RequiredDependency,
/// 4 Tool, 5 Incompatible, 6 Include.
fn dependency_kind(t: u8) -> &'static str {
    match t {
        3 => "required",
        2 => "optional",
        5 => "incompatible",
        _ => "other",
    }
}

impl RawMod {
    fn into_search_result(self) -> SearchResult {
        // CurseForge fuehrt Loader nicht als eigenes Feld am Mod, nur je
        // Datei -- hier aus den indizierten Fassungen zusammengetragen.
        let loaders: Vec<String> = self
            .latest_files_indexes
            .iter()
            .filter_map(|f| f.mod_loader)
            .map(|l| match l {
                1 => "forge",
                4 => "fabric",
                5 => "quilt",
                6 => "neoforge",
                _ => "andere",
            })
            .map(str::to_string)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        let game_versions: Vec<String> = self
            .latest_files_indexes
            .iter()
            .map(|f| f.game_version.clone())
            .filter(|v| !v.is_empty())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        SearchResult {
            provider: "curseforge".into(),
            project_id: self.id.to_string(),
            slug: self.slug,
            title: self.name,
            description: self.summary,
            author: self.authors.first().map(|a| a.name.clone()).unwrap_or_default(),
            downloads: self.download_count,
            follows: 0, // CurseForge fuehrt keine vergleichbare Kennzahl.
            icon_url: self.logo.and_then(|l| l.thumbnail_url),
            categories: self.categories.into_iter().map(|c| c.name).collect(),
            project_type: "mod".into(),
            loaders,
            game_versions,
            updated_at: self.date_modified,
            license: None, // Nicht Teil der Suchantwort, nur der Einzelabfrage.
            client_side: "unknown".into(),
            server_side: "unknown".into(),
        }
    }
}

impl RawFile {
    fn into_mod_version(self) -> Option<ModVersion> {
        let url = self.download_url?;
        let sha1 = self.hashes.iter().find(|h| h.algo == 1).map(|h| h.value.clone());

        let loaders = self.game_versions.iter()
            .map(|v| v.to_ascii_lowercase())
            .filter(|v| loader_type(v).is_some())
            .collect();

        Some(ModVersion {
            // CurseForge braucht fuer eine Einzelabfrage sowohl Mod- als
            // auch Dateikennung -- anders als bei Modrinth, wo eine
            // Fassung global eindeutig ist. Beide zusammen kodiert, damit
            // `version()` unten sie wieder auseinandernehmen kann, ohne
            // dass der Aufrufer sich den Mod separat merken muss.
            version_id: format!("{}:{}", self.mod_id, self.id),
            project_id: self.mod_id.to_string(),
            provider: "curseforge".into(),
            name: self.display_name,
            version_number: self.file_name.clone(),
            changelog: None, // Eigener Endpunkt, hier nicht abgefragt.
            release_type: release_type_name(self.release_type).into(),
            downloads: self.download_count,
            published_at: self.file_date,
            game_versions: self.game_versions,
            loaders,
            filename: self.file_name,
            url,
            size_bytes: self.file_length,
            sha1,
            dependencies: self
                .dependencies
                .into_iter()
                .map(|d| ModDependency {
                    project_id: Some(d.mod_id.to_string()),
                    version_id: None,
                    kind: dependency_kind(d.relation_type).into(),
                })
                .collect(),
        })
    }
}

/* ---- Anfragen ------------------------------------------------------------ */

/// Sucht bei CurseForge. Ohne Schluessel eine leere Seite, kein Fehler --
/// der Anbieter ist optional.
///
/// Nur `project_type == "mod"` wird gefiltert (`classId` fuer die anderen
/// Typen ist nicht verifiziert, siehe Moduldoku); andere Typen laufen ohne
/// Klassenfilter durch, was breitere, aber nicht falsche Treffer ergibt.
pub async fn search_in(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    project_type: &str,
    page: u32,
) -> Result<SearchPage> {
    // Ohne verifizierte Shader-Klasse würden hier normale Mods in der
    // Shader-Suche erscheinen. Shaderpakete liefert Modrinth.
    if project_type == "shader" {
        return Ok(SearchPage { results: Vec::new(), total: 0 });
    }
    let Some(key) = crate::ai::keys::curseforge_api_key() else {
        return Ok(SearchPage { results: Vec::new(), total: 0 });
    };

    const PER_PAGE: u32 = 20;
    let mut url = format!(
        "{BASE}/mods/search?gameId={MINECRAFT_GAME_ID}&pageSize={PER_PAGE}&index={}",
        page * PER_PAGE,
    );
    if !query.trim().is_empty() {
        url.push_str(&format!("&searchFilter={}", urlencode(query)));
    }
    if project_type == "mod" {
        url.push_str(&format!("&classId={MOD_CLASS_ID}"));
    }
    if let Some(v) = game_version.filter(|v| !v.is_empty()) {
        url.push_str(&format!("&gameVersion={}", urlencode(v)));
    }
    if let Some(l) = loader.and_then(loader_type) {
        url.push_str(&format!("&modLoaderType={l}"));
    }

    let raw: SearchResponse = http::get_json_with_key(&url, HEADER, &key).await?;
    Ok(SearchPage {
        results: raw.data.into_iter().map(RawMod::into_search_result).collect(),
        total: raw.pagination.total_count,
    })
}

/// Die Fassungen eines Mods, passend gefiltert. Leer ohne Schluessel.
pub async fn versions(
    project: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModVersion>> {
    let Some(key) = crate::ai::keys::curseforge_api_key() else {
        return Ok(Vec::new());
    };
    let Ok(mod_id) = project.parse::<u64>() else {
        // Kennungen sind bei CurseForge Zahlen -- ein Modrinth-Slug landet
        // hier nie, das ist kein Fehler, nur kein Treffer.
        return Ok(Vec::new());
    };

    let mut url = format!("{BASE}/mods/{mod_id}/files?pageSize=50");
    if let Some(v) = game_version.filter(|v| !v.is_empty()) {
        url.push_str(&format!("&gameVersion={}", urlencode(v)));
    }
    if let Some(l) = loader.and_then(loader_type) {
        url.push_str(&format!("&modLoaderType={l}"));
    }

    let raw: FilesResponse = http::get_json_with_key(&url, HEADER, &key).await.unwrap_or(
        FilesResponse { data: Vec::new() },
    );
    Ok(raw.data.into_iter().filter_map(RawFile::into_mod_version)
        .filter(|v| crate::modrinth::matches_target(v, game_version, loader)).collect())
}

#[derive(Debug, Clone, Deserialize)]
struct FileResponse {
    data: RawFile,
}

/// Eine einzelne Fassung -- `version_id` muss die von `versions()`
/// gelieferte Form `"{mod_id}:{file_id}"` haben, sonst gibt es keinen
/// Treffer statt eines Fehlers (die Kennung stammt dann nicht von hier).
pub async fn version(version_id: &str) -> Result<ModVersion> {
    let Some((mod_id, file_id)) = version_id.split_once(':') else {
        return Err(crate::Error::not_found("Keine CurseForge-Fassungskennung"));
    };
    let key = crate::ai::keys::curseforge_api_key()
        .ok_or_else(|| crate::Error::not_found("Kein CurseForge-Schlüssel hinterlegt"))?;

    let url = format!("{BASE}/mods/{mod_id}/files/{file_id}");
    let raw: FileResponse = http::get_json_with_key(&url, HEADER, &key).await?;
    raw.data
        .into_mod_version()
        .ok_or_else(|| crate::Error::not_found("Diese Fassung hat keine herunterladbare Datei"))
}

/// Titel und Symbol eines einzelnen Mods -- fuer Sammelabfragen wie bei
/// Modrinth gibt es hier keinen Batch-Endpunkt, deshalb einzeln.
pub async fn project(id: &str) -> Result<(String, Option<String>)> {
    let key = crate::ai::keys::curseforge_api_key()
        .ok_or_else(|| crate::Error::not_found("Kein CurseForge-Schlüssel hinterlegt"))?;
    let raw: ModResponse = http::get_json_with_key(&format!("{BASE}/mods/{id}"), HEADER, &key).await?;
    Ok((raw.data.name, raw.data.logo.and_then(|l| l.thumbnail_url)))
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadersuche_liefert_keine_normalen_mods() {
        let result = tokio_test_block_on(search_in("", Some("1.21.1"), Some("buildmc"), "shader", 0)).unwrap();
        assert!(result.results.is_empty());
        assert_eq!(result.total, 0);
    }

    #[test]
    fn loader_zahlen_stimmen_mit_der_dokumentation_ueberein() {
        assert_eq!(loader_type("forge"), Some(1));
        assert_eq!(loader_type("fabric"), Some(4));
        assert_eq!(loader_type("quilt"), Some(5));
        assert_eq!(loader_type("neoforge"), Some(6));
        assert_eq!(loader_type("vanilla"), None);
        assert_eq!(loader_type("buildmc"), Some(4));
    }

    #[test]
    fn freigabetypen_werden_uebersetzt() {
        assert_eq!(release_type_name(1), "release");
        assert_eq!(release_type_name(2), "beta");
        assert_eq!(release_type_name(3), "alpha");
    }

    #[test]
    fn abhaengigkeitsarten_werden_uebersetzt() {
        assert_eq!(dependency_kind(3), "required");
        assert_eq!(dependency_kind(2), "optional");
        assert_eq!(dependency_kind(5), "incompatible");
    }

    fn beispiel_mod() -> RawMod {
        RawMod {
            id: 238222,
            name: "Sodium".into(),
            slug: "sodium".into(),
            summary: "Ein Renderer-Ersatz.".into(),
            download_count: 50_000_000,
            logo: Some(RawLogo { thumbnail_url: Some("https://example.com/icon.png".into()) }),
            categories: vec![RawCategory { name: "Optimization".into() }],
            authors: vec![RawAuthor { name: "jellysquid3".into() }],
            latest_files_indexes: vec![
                RawFileIndex { game_version: "1.21.4".into(), mod_loader: Some(4) },
                RawFileIndex { game_version: "1.21.1".into(), mod_loader: Some(4) },
            ],
            date_modified: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn suchtreffer_traegt_den_anbieter_curseforge() {
        let r = beispiel_mod().into_search_result();
        assert_eq!(r.provider, "curseforge");
        assert_eq!(r.title, "Sodium");
        assert_eq!(r.author, "jellysquid3");
        assert!(r.loaders.contains(&"fabric".to_string()));
        assert!(r.game_versions.contains(&"1.21.4".to_string()));
    }

    fn beispiel_datei() -> RawFile {
        RawFile {
            id: 1234,
            mod_id: 238222,
            display_name: "sodium-fabric-0.6.13.jar".into(),
            file_name: "sodium-fabric-0.6.13+mc1.21.4.jar".into(),
            file_date: "2026-01-01T00:00:00Z".into(),
            download_count: 1000,
            file_length: 123_456,
            release_type: 1,
            download_url: Some("https://edge.forgecdn.net/files/1/234/sodium.jar".into()),
            game_versions: vec!["1.21.4".into(), "Fabric".into()],
            hashes: vec![RawHash { value: "deadbeef".into(), algo: 1 }],
            dependencies: vec![RawFileDependency { mod_id: 306612, relation_type: 3 }],
        }
    }

    #[test]
    fn dateiantwort_traegt_sha1_und_abhaengigkeiten() {
        let v = beispiel_datei().into_mod_version().unwrap();
        assert_eq!(v.provider, "curseforge");
        assert_eq!(v.loaders, vec!["fabric"]);
        assert!(crate::modrinth::matches_target(&v, Some("1.21.4"), Some("buildmc")));
        for loader in ["forge", "neoforge", "quilt"] {
            assert!(!crate::modrinth::matches_target(&v, Some("1.21.4"), Some(loader)));
        }
        assert_eq!(v.sha1.as_deref(), Some("deadbeef"));
        assert_eq!(v.dependencies.len(), 1);
        assert_eq!(v.dependencies[0].kind, "required");
    }

    /// Anders als bei Modrinth ist eine Fassung bei CurseForge nicht
    /// global eindeutig -- die Einzelabfrage braucht Mod- *und*
    /// Dateikennung. Beide werden deshalb zusammen kodiert.
    #[test]
    fn versionskennung_traegt_mod_und_dateikennung_zusammen() {
        let v = beispiel_datei().into_mod_version().unwrap();
        assert_eq!(v.version_id, "238222:1234");
    }

    #[test]
    fn version_ohne_doppelpunkt_ergibt_einen_klaren_fehler() {
        let ergebnis = tokio_test_block_on(version("keine-gueltige-kennung"));
        assert!(ergebnis.is_err());
    }

    #[test]
    fn datei_ohne_download_adresse_ergibt_keine_fassung() {
        let mut datei = beispiel_datei();
        datei.download_url = None;
        assert!(datei.into_mod_version().is_none());
    }

    #[test]
    fn projekt_kennung_die_keine_zahl_ist_ergibt_leer_statt_fehler() {
        // Ein Modrinth-Slug landet nie bei CurseForge -- das ist kein
        // Fehlerfall, nur "hier gibt es nichts".
        let result = tokio_test_block_on(versions("sodium-slug-kein-zahlwert", None, None));
        assert!(result.unwrap().is_empty());
    }

    // Winziger Ersatz fuer #[tokio::test], um keine neue Abhaengigkeit nur
    // fuer einen synchronen Fruehausstieg-Test zu brauchen.
    fn tokio_test_block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(f)
    }
}
