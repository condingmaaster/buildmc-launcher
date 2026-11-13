//! Rollen gegen Modrinth aufloesen.
//!
//! Fuer jede Rolle werden die Anwaerter der Reihe nach gefragt; der erste,
//! den es fuer diese Minecraft-Version gibt, gewinnt. Bevorzugt werden
//! endgueltige Fassungen -- fuer eine frische Instanz ist eine Beta die
//! schlechtere Wahl, auch wenn sie neuer ist.

use serde::{Deserialize, Serialize};

use super::roles::{self, Role};
use super::Level;
use crate::error::Result;
use crate::http;

#[derive(Debug, Clone, Deserialize)]
struct ModrinthVersion {
    id: String,
    name: String,
    version_number: String,
    version_type: String,
    files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Deserialize)]
struct ModrinthFile {
    url: String,
    filename: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    primary: bool,
    #[serde(default)]
    hashes: std::collections::HashMap<String, String>,
}

/// Was fuer eine Rolle gefunden wurde.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedMod {
    pub slug: String,
    pub version_id: String,
    pub version_number: String,
    pub version_type: String,
    pub filename: String,
    pub url: String,
    pub size: u64,
    pub sha1: Option<String>,
}

/// Das Ergebnis je Rolle -- auch wenn nichts gefunden wurde.
#[derive(Debug, Clone, Serialize)]
pub struct ResolvedRole {
    pub role_id: String,
    pub label: String,
    pub why: String,
    pub optional: bool,
    /// Der Gewinner, sofern es einen gibt.
    pub chosen: Option<ResolvedMod>,
    /// Welche Anwaerter erfolglos befragt wurden.
    pub skipped: Vec<String>,
    /// Erlaeuterung, wenn nichts gefunden wurde.
    pub note: Option<String>,
}

/// Der gesamte Plan fuer eine Stufe.
#[derive(Debug, Clone, Serialize)]
pub struct OptimizationPlan {
    pub mc_version: String,
    pub loader: String,
    pub level: Level,
    pub roles: Vec<ResolvedRole>,
    pub total_bytes: u64,
    /// Ist eine unverzichtbare Rolle ausgefallen?
    pub usable: bool,
}

/// Fragt Modrinth nach den Fassungen eines Projekts.
async fn versions_of(slug: &str, mc_version: &str, loader: &str) -> Result<Vec<ModrinthVersion>> {
    let url = format!(
        "https://api.modrinth.com/v2/project/{slug}/version\
         ?game_versions=%5B%22{mc_version}%22%5D&loaders=%5B%22{loader}%22%5D"
    );
    // Ein unbekanntes Projekt ist kein Fehler des Nutzers -- dann gilt die
    // Rolle als nicht besetzbar und der naechste Anwaerter kommt dran.
    Ok(http::get_json::<Vec<ModrinthVersion>>(&url).await.unwrap_or_default())
}

/// Endgueltige Fassungen zuerst, dann Beta, dann Alpha.
fn best(mut versions: Vec<ModrinthVersion>) -> Option<ModrinthVersion> {
    let rank = |t: &str| match t {
        "release" => 0,
        "beta" => 1,
        _ => 2,
    };
    versions.sort_by_key(|v| rank(&v.version_type));
    versions.into_iter().next()
}

fn into_resolved(slug: &str, v: ModrinthVersion) -> Option<ResolvedMod> {
    // Die Hauptdatei ist die Mod selbst; daneben liegen oft Quellen-Jars.
    let file = v
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| v.files.first())?
        .clone();

    Some(ResolvedMod {
        slug: slug.to_string(),
        version_id: v.id,
        version_number: if v.version_number.is_empty() { v.name } else { v.version_number },
        version_type: v.version_type,
        filename: file.filename,
        url: file.url,
        size: file.size,
        sha1: file.hashes.get("sha1").cloned(),
    })
}

async fn resolve_role(role: &Role, mc_version: &str, loader: &str) -> ResolvedRole {
    let mut skipped = Vec::new();

    for candidate in role.candidates {
        let found = versions_of(candidate, mc_version, loader).await.unwrap_or_default();
        if let Some(chosen) = best(found).and_then(|v| into_resolved(candidate, v)) {
            return ResolvedRole {
                role_id: role.id.into(),
                label: role.label.into(),
                why: role.why.into(),
                optional: role.optional,
                chosen: Some(chosen),
                skipped,
                note: None,
            };
        }
        skipped.push((*candidate).to_string());
    }

    ResolvedRole {
        role_id: role.id.into(),
        label: role.label.into(),
        why: role.why.into(),
        optional: role.optional,
        chosen: None,
        note: Some(if role.optional {
            format!("Für Minecraft {mc_version} nicht verfügbar — wird ausgelassen.")
        } else {
            format!("Für Minecraft {mc_version} nicht verfügbar. Ohne diese Rolle \
                     bringt die Stufe deutlich weniger.")
        }),
        skipped,
    }
}

/// Stellt den Plan fuer eine Minecraft-Version zusammen.
pub async fn plan(mc_version: &str, loader: &str, level: Level) -> Result<OptimizationPlan> {
    let mut resolved = Vec::new();

    for role in roles::for_level(level) {
        resolved.push(resolve_role(role, mc_version, loader).await);
    }

    let total_bytes = resolved
        .iter()
        .filter_map(|r| r.chosen.as_ref().map(|c| c.size))
        .sum();

    // Brauchbar ist der Plan, solange keine unverzichtbare Rolle fehlt.
    let usable = resolved.iter().all(|r| r.optional || r.chosen.is_some());

    Ok(OptimizationPlan {
        mc_version: mc_version.to_string(),
        loader: loader.to_string(),
        level,
        roles: resolved,
        total_bytes,
        usable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(t: &str, num: &str) -> ModrinthVersion {
        ModrinthVersion {
            id: format!("id-{num}"),
            name: num.into(),
            version_number: num.into(),
            version_type: t.into(),
            files: vec![ModrinthFile {
                url: "https://example.org/x.jar".into(),
                filename: "x.jar".into(),
                size: 1000,
                primary: true,
                hashes: Default::default(),
            }],
        }
    }

    #[test]
    fn endgueltige_fassung_schlaegt_beta() {
        let picked = best(vec![
            version("beta", "0.9-beta"),
            version("release", "0.8"),
            version("alpha", "1.0-alpha"),
        ])
        .unwrap();
        assert_eq!(picked.version_type, "release");
        assert_eq!(picked.version_number, "0.8");
    }

    #[test]
    fn beta_wenn_es_nichts_endgueltiges_gibt() {
        let picked = best(vec![version("alpha", "a"), version("beta", "b")]).unwrap();
        assert_eq!(picked.version_type, "beta");
    }

    #[test]
    fn ohne_fassungen_kein_ergebnis() {
        assert!(best(vec![]).is_none());
    }

    #[test]
    fn hauptdatei_wird_bevorzugt() {
        let mut v = version("release", "1.0");
        v.files.insert(0, ModrinthFile {
            url: "https://example.org/sources.jar".into(),
            filename: "sources.jar".into(),
            size: 50,
            primary: false,
            hashes: Default::default(),
        });
        let r = into_resolved("test", v).unwrap();
        assert_eq!(r.filename, "x.jar", "die Quellen-Jar darf es nicht sein");
    }
}
