//! Unsere Anbindung an Modrinth.
//!
//! Modrinth verteilt die Mod-Dateien; dieses Modul stellt die Anfragen und
//! uebersetzt die Antworten in unsere eigenen Typen. Nichts davon ist
//! fremder Code -- es ist die Stelle, an der unser Launcher mit dem
//! Portal spricht, und die einzige, die dessen Datenformat kennt.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::http;

const BASE: &str = "https://api.modrinth.com/v2";

/* ---- Rohformat von Modrinth ------------------------------------------- */

#[derive(Debug, Clone, Deserialize)]
struct SearchResponse {
    hits: Vec<Hit>,
    total_hits: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct Hit {
    project_id: String,
    slug: String,
    title: String,
    description: String,
    author: String,
    downloads: u64,
    follows: u64,
    icon_url: Option<String>,
    categories: Vec<String>,
    project_type: String,
    versions: Vec<String>,
    date_modified: String,
    license: Option<String>,
    client_side: String,
    server_side: String,
}

#[derive(Debug, Clone, Deserialize)]
struct Project {
    id: String,
    slug: String,
    title: String,
    icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawVersion {
    id: String,
    project_id: String,
    name: String,
    version_number: String,
    version_type: String,
    changelog: Option<String>,
    downloads: u64,
    date_published: String,
    game_versions: Vec<String>,
    loaders: Vec<String>,
    files: Vec<RawFile>,
    #[serde(default)]
    dependencies: Vec<RawDependency>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawFile {
    url: String,
    filename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: HashMap<String, String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawDependency {
    #[serde(default)]
    project_id: Option<String>,
    #[serde(default)]
    version_id: Option<String>,
    dependency_type: String,
}

/* ---- Unsere Typen ------------------------------------------------------ */

/// Loader-Bezeichnungen, die Modrinth als Kategorie fuehrt.
const LOADER_TAGS: [&str; 4] = ["fabric", "forge", "neoforge", "quilt"];

/// BuildMC startet mit Fabric und muss dessen Mod-Dateien verwenden.
/// Forge, NeoForge und Quilt bleiben eigenständige Ziele.
pub fn mod_loader(loader: &str) -> &str {
    match loader {
        "buildmc" => "fabric",
        other => other,
    }
}

/// Portalfilter sind keine Garantie: Antworten vor der Auswahl nachprüfen.
pub(crate) fn matches_target(
    version: &ModVersion,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> bool {
    let game_matches = game_version.filter(|v| !v.is_empty())
        .map_or(true, |v| version.game_versions.iter().any(|g| g == v));
    let loader_matches = loader.map(mod_loader).filter(|l| LOADER_TAGS.contains(l))
        .map_or(true, |l| version.loaders.iter().any(|v| v == l));
    game_matches && loader_matches
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub provider: String,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub author: String,
    pub downloads: u64,
    pub follows: u64,
    pub icon_url: Option<String>,
    pub categories: Vec<String>,
    pub project_type: String,
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub updated_at: String,
    pub license: Option<String>,
    pub client_side: String,
    pub server_side: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModVersion {
    pub version_id: String,
    pub project_id: String,
    pub provider: String,
    pub name: String,
    pub version_number: String,
    pub changelog: Option<String>,
    pub release_type: String,
    pub downloads: u64,
    pub published_at: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    pub filename: String,
    pub url: String,
    pub size_bytes: u64,
    pub sha1: Option<String>,
    pub dependencies: Vec<ModDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModDependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchPage {
    pub results: Vec<SearchResult>,
    pub total: u32,
}

impl From<Hit> for SearchResult {
    fn from(h: Hit) -> Self {
        // Modrinth mischt die Loader unter die Kategorien -- hier trennen,
        // damit die Oberflaeche beides getrennt anzeigen kann.
        let (loaders, categories): (Vec<String>, Vec<String>) = h
            .categories
            .into_iter()
            .partition(|c| LOADER_TAGS.contains(&c.as_str()));

        SearchResult {
            provider: "modrinth".into(),
            project_id: h.project_id,
            slug: h.slug,
            title: h.title,
            description: h.description,
            author: h.author,
            downloads: h.downloads,
            follows: h.follows,
            icon_url: h.icon_url.filter(|s| !s.is_empty()),
            categories,
            project_type: h.project_type,
            loaders,
            game_versions: h.versions,
            updated_at: h.date_modified,
            license: h.license,
            client_side: h.client_side,
            server_side: h.server_side,
        }
    }
}

impl RawVersion {
    fn into_mod_version(self) -> Option<ModVersion> {
        // Die Hauptdatei ist die Mod; daneben liegen oft Quellen-Jars.
        let file = self
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())?
            .clone();

        Some(ModVersion {
            version_id: self.id,
            project_id: self.project_id,
            provider: "modrinth".into(),
            name: self.name,
            version_number: self.version_number,
            changelog: self.changelog.filter(|c| !c.trim().is_empty()),
            release_type: self.version_type,
            downloads: self.downloads,
            published_at: self.date_published,
            game_versions: self.game_versions,
            loaders: self.loaders,
            filename: file.filename,
            url: file.url,
            size_bytes: file.size,
            sha1: file.hashes.get("sha1").cloned(),
            dependencies: self
                .dependencies
                .into_iter()
                .map(|d| ModDependency {
                    project_id: d.project_id,
                    version_id: d.version_id,
                    kind: d.dependency_type,
                })
                .collect(),
        })
    }
}

/* ---- Anfragen ---------------------------------------------------------- */

fn facets(
    project_type: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    categories: &[String],
) -> String {
    let mut groups: Vec<String> = vec![format!("[\"project_type:{project_type}\"]")];

    // Kategorien sind eine eigene Gruppe: Modrinth verknüpft innerhalb
    // einer Gruppe mit ODER, zwischen Gruppen mit UND.
    if !categories.is_empty() {
        let list: Vec<String> = categories
            .iter()
            .map(|c| format!("\"categories:{c}\""))
            .collect();
        groups.push(format!("[{}]", list.join(",")));
    }
    if let Some(v) = game_version.filter(|v| !v.is_empty()) {
        groups.push(format!("[\"versions:{v}\"]"));
    }
    // BuildMC verwendet Fabric; Vanilla hat keinen Portal-Loader.
    if let Some(l) = loader.map(mod_loader)
        .filter(|l| project_type != "shader" && LOADER_TAGS.contains(l)) {
        groups.push(format!("[\"categories:{l}\"]"));
    }
    format!("[{}]", groups.join(","))
}

pub async fn search(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    project_type: &str,
    page: u32,
) -> Result<SearchPage> {
    search_in(query, game_version, loader, project_type, &[], page).await
}

/// Suche mit Kategorien.
///
/// Der Unterschied ist erheblich: „optimization" als Volltext findet Mods,
/// die das Wort im Namen tragen; als Kategorie findet es die, die
/// tatsaechlich optimieren. Wo Modrinth eine Kategorie fuehrt, ist sie
/// immer die bessere Wahl.
pub async fn search_in(
    query: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
    project_type: &str,
    categories: &[String],
    page: u32,
) -> Result<SearchPage> {
    const PER_PAGE: u32 = 20;
    // Ohne Suchwort nach Beliebtheit ordnen -- sonst kaeme eine
    // willkuerliche Reihenfolge.
    let index = if query.trim().is_empty() { "&index=downloads" } else { "" };
    let url = format!(
        "{BASE}/search?query={}&limit={PER_PAGE}&offset={}{index}&facets={}",
        urlencode(query),
        page * PER_PAGE,
        urlencode(&facets(project_type, game_version, loader, categories)),
    );

    let raw: SearchResponse = http::get_json(&url).await?;
    Ok(SearchPage {
        results: raw.hits.into_iter().map(SearchResult::from).collect(),
        total: raw.total_hits,
    })
}

/// Die Fassungen eines Projekts, passend gefiltert.
pub async fn versions(
    project: &str,
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<ModVersion>> {
    let mut url = format!("{BASE}/project/{project}/version");
    let mut params: Vec<String> = Vec::new();
    if let Some(v) = game_version.filter(|v| !v.is_empty()) {
        params.push(format!("game_versions={}", urlencode(&format!("[\"{v}\"]"))));
    }
    if let Some(l) = loader.map(mod_loader).filter(|l| LOADER_TAGS.contains(l)) {
        params.push(format!("loaders={}", urlencode(&format!("[\"{l}\"]"))));
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let raw: Vec<RawVersion> = http::get_json(&url).await.unwrap_or_default();
    Ok(raw.into_iter().filter_map(RawVersion::into_mod_version)
        .filter(|v| matches_target(v, game_version, loader)).collect())
}

/// Eine einzelne Fassung.
pub async fn version(version_id: &str) -> Result<ModVersion> {
    let raw: RawVersion = http::get_json(&format!("{BASE}/version/{version_id}")).await?;
    raw.into_mod_version()
        .ok_or_else(|| crate::Error::not_found("Diese Fassung hat keine Datei"))
}

/// Symbole und Titel mehrerer Projekte in einem Aufruf.
///
/// Ein Aufruf je Mod waere bei hundert Eintraegen ein Sturm von Anfragen.
/// Modrinth beantwortet bis zu mehrere hundert Kennungen auf einmal.
pub async fn projects(ids: &[String]) -> Result<HashMap<String, (String, Option<String>)>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let list = format!("[{}]", ids.iter().map(|i| format!("\"{i}\"")).collect::<Vec<_>>().join(","));
    let url = format!("{BASE}/projects?ids={}", urlencode(&list));

    let raw: Vec<Project> = http::get_json(&url).await.unwrap_or_default();
    let mut out = HashMap::new();
    for p in raw {
        let entry = (p.title, p.icon_url.filter(|s| !s.is_empty()));
        // Unter beiden Schluesseln ablegen: mal liegt die Kennung vor,
        // mal der Namensteil aus der Adresse.
        out.insert(p.id, entry.clone());
        out.insert(p.slug, entry);
    }
    Ok(out)
}

/// Prozentkodierung fuer Abfrageparameter.
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shadersuche_hat_keinen_mod_loaderfilter() {
        for loader in ["fabric", "buildmc", "quilt", "forge", "neoforge"] {
            let f = facets("shader", Some("1.21.1"), Some(loader), &[]);
            assert!(f.contains("project_type:shader"));
            assert!(f.contains("versions:1.21.1"));
            assert!(!f.contains("categories:"));
        }
    }

    #[test]
    fn buildmc_suche_verwendet_fabric_filter() {
        assert_eq!(facets("mod", Some("1.21.1"), Some("buildmc"), &[]),
                   facets("mod", Some("1.21.1"), Some("fabric"), &[]));
        for loader in LOADER_TAGS {
            assert_eq!(mod_loader(loader), loader);
        }
    }

    #[test]
    fn kodierung_schont_erlaubte_zeichen() {
        assert_eq!(urlencode("sodium"), "sodium");
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("[\"fabric\"]"), "%5B%22fabric%22%5D");
    }

    #[test]
    fn facetten_lassen_vanilla_aus() {
        // Vanilla und buildmc kennt Modrinth nicht als Loader.
        let f = facets("mod", Some("1.21.4"), Some("vanilla"), &[]);
        assert!(!f.contains("categories"));
        assert!(f.contains("versions:1.21.4"));

        let f = facets("mod", Some("1.21.4"), Some("fabric"), &[]);
        assert!(f.contains("categories:fabric"));
    }

    #[test]
    fn leere_version_wird_weggelassen() {
        let f = facets("shader", Some(""), None, &[]);
        assert_eq!(f, "[[\"project_type:shader\"]]");
    }

    #[test]
    fn kategorien_bilden_eine_eigene_gruppe() {
        // Innerhalb einer Gruppe ODER, zwischen Gruppen UND.
        let f = facets("mod", None, None, &["optimization".into(), "utility".into()]);
        assert!(f.contains("[\"categories:optimization\",\"categories:utility\"]"), "{f}");
    }

    #[test]
    fn ohne_kategorien_keine_leere_gruppe() {
        let f = facets("mod", None, None, &[]);
        assert!(!f.contains("categories"));
    }

    #[test]
    fn loader_werden_aus_den_kategorien_getrennt() {
        let hit = Hit {
            project_id: "x".into(), slug: "s".into(), title: "T".into(),
            description: "d".into(), author: "a".into(),
            downloads: 1, follows: 1, icon_url: Some("".into()),
            categories: vec!["fabric".into(), "optimization".into(), "quilt".into()],
            project_type: "mod".into(), versions: vec![],
            date_modified: "".into(), license: None,
            client_side: "required".into(), server_side: "unsupported".into(),
        };
        let r = SearchResult::from(hit);
        assert_eq!(r.loaders, vec!["fabric", "quilt"]);
        assert_eq!(r.categories, vec!["optimization"]);
        // Leere Symboladresse gilt als keine.
        assert!(r.icon_url.is_none());
    }
}
