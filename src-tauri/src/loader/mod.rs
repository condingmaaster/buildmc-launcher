//! Mod-Loader installieren.
//!
//! Fabric und Quilt stellen eine Meta-API bereit, die ein fertiges
//! Versionsprofil liefert: eine Versions-JSON mit `inheritsFrom`, die von
//! der Grundversion erbt und nur die eigenen Bibliotheken und Argumente
//! ergaenzt. Installieren heisst deshalb schlicht: Profil holen, ablegen.
//! Den Rest erledigt die vorhandene Startkette, die Vererbung ohnehin
//! aufloest.
//!
//! **Forge und NeoForge gehen diesen Weg nicht.** Sie liefern kein Profil,
//! sondern einen Installer-Jar, der ausgefuehrt werden muss und dabei die
//! Minecraft-Jar umschreibt. Das ist ein eigener Vorgang mit eigenen
//! Fallstricken und wird hier bewusst nicht vorgetaeuscht.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::{http, paths};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LoaderKind {
    Fabric,
    Quilt,
    NeoForge,
    Forge,
}

impl LoaderKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "fabric" => Some(Self::Fabric),
            "quilt" => Some(Self::Quilt),
            "neoforge" => Some(Self::NeoForge),
            "forge" => Some(Self::Forge),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Fabric => "Fabric",
            Self::Quilt => "Quilt",
            Self::NeoForge => "NeoForge",
            Self::Forge => "Forge",
        }
    }

    /// Nur diese beiden lassen sich ueber ein Profil installieren.
    fn meta_base(self) -> Option<&'static str> {
        match self {
            Self::Fabric => Some("https://meta.fabricmc.net/v2"),
            Self::Quilt => Some("https://meta.quiltmc.org/v3"),
            _ => None,
        }
    }

    /// Wie das Versionsverzeichnis heisst.
    ///
    /// Die Benennung stammt aus dem offiziellen Installer und wird von
    /// allen Launchern so erwartet.
    pub fn version_id(self, loader_version: &str, mc_version: &str) -> String {
        match self {
            Self::Fabric => format!("fabric-loader-{loader_version}-{mc_version}"),
            Self::Quilt => format!("quilt-loader-{loader_version}-{mc_version}"),
            Self::NeoForge => format!("neoforge-{loader_version}"),
            Self::Forge => format!("{mc_version}-forge-{loader_version}"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct LoaderVersion {
    pub version: String,
    /// Als endgueltig gekennzeichnet. Voreinstellung sollte immer eine
    /// stabile Fassung sein.
    pub stable: bool,
    /// Bereits abgelegt?
    pub installed: bool,
}

/* ---- Rohformat der Meta-APIs ------------------------------------------ */

#[derive(Debug, Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}

#[derive(Debug, Deserialize)]
struct MetaLoader {
    version: String,
    #[serde(default)]
    stable: bool,
}

/// Die verfuegbaren Loader-Fassungen fuer eine Minecraft-Version.
///
/// Die Liste kommt neueste zuerst; stabile werden vorgezogen, damit die
/// Voreinstellung in der Oberflaeche nicht zufaellig eine Vorabfassung ist.
pub async fn versions(kind: LoaderKind, mc_version: &str) -> Result<Vec<LoaderVersion>> {
    let Some(base) = kind.meta_base() else {
        return Err(unsupported(kind));
    };

    let raw: Vec<MetaEntry> =
        http::get_json(&format!("{base}/versions/loader/{mc_version}")).await?;

    let mut out: Vec<LoaderVersion> = raw
        .into_iter()
        .map(|e| {
            let id = kind.version_id(&e.loader.version, mc_version);
            LoaderVersion {
                installed: paths::version_json(&id).map(|p| p.exists()).unwrap_or(false),
                version: e.loader.version,
                stable: e.loader.stable,
            }
        })
        .collect();

    // Stabile zuerst, Reihenfolge innerhalb davon unveraendert (neueste oben).
    out.sort_by_key(|v| !v.stable);
    Ok(out)
}

/// Die empfohlene Fassung: die neueste stabile.
pub async fn recommended(kind: LoaderKind, mc_version: &str) -> Result<String> {
    let list = versions(kind, mc_version).await?;
    list.iter()
        .find(|v| v.stable)
        .or_else(|| list.first())
        .map(|v| v.version.clone())
        .ok_or_else(|| {
            Error::not_found(format!(
                "Für Minecraft {mc_version} gibt es keine {}-Fassung",
                kind.label()
            ))
        })
}

/// Holt das Profil und legt es ab.
///
/// Gibt die Versions-Id zurueck, unter der gestartet wird.
pub async fn install(kind: LoaderKind, mc_version: &str, loader_version: &str) -> Result<String> {
    let Some(base) = kind.meta_base() else {
        return Err(unsupported(kind));
    };

    let id = kind.version_id(loader_version, mc_version);
    let target = paths::version_json(&id)?;
    if target.exists() {
        return Ok(id);
    }

    let url = format!("{base}/versions/loader/{mc_version}/{loader_version}/profile/json");
    let bytes = http::get_bytes(&url).await.map_err(|e| {
        Error::other(format!(
            "{} {loader_version} für Minecraft {mc_version} ließ sich nicht laden: {e}",
            kind.label()
        ))
    })?;

    // Vor dem Ablegen pruefen, ob es wirklich ein Profil ist -- eine
    // Fehlerseite waere sonst als Versions-JSON abgelegt und fiele erst
    // beim Start auf.
    let parsed: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| Error::other("Die Antwort ist kein gültiges Versionsprofil"))?;
    if parsed.get("inheritsFrom").is_none() || parsed.get("mainClass").is_none() {
        return Err(Error::other(
            "Die Antwort sieht nicht wie ein Loader-Profil aus",
        ));
    }

    paths::ensure_parent(&target)?;
    std::fs::write(&target, &bytes)?;
    Ok(id)
}

fn unsupported(kind: LoaderKind) -> Error {
    Error::other(format!(
        "{} lässt sich noch nicht installieren. Anders als Fabric und Quilt \
         liefert es kein fertiges Versionsprofil, sondern einen Installer, \
         der ausgeführt werden muss und dabei die Minecraft-Datei umschreibt. \
         Das ist ein eigener Vorgang und noch nicht gebaut.",
        kind.label()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_ids_folgen_der_ueblichen_benennung() {
        assert_eq!(
            LoaderKind::Fabric.version_id("0.19.5", "1.21.4"),
            "fabric-loader-0.19.5-1.21.4"
        );
        assert_eq!(
            LoaderKind::Quilt.version_id("0.29.1", "1.21.4"),
            "quilt-loader-0.29.1-1.21.4"
        );
        assert_eq!(
            LoaderKind::Forge.version_id("47.2.0", "1.20.1"),
            "1.20.1-forge-47.2.0"
        );
    }

    #[test]
    fn kennungen_werden_erkannt() {
        assert_eq!(LoaderKind::parse("fabric"), Some(LoaderKind::Fabric));
        assert_eq!(LoaderKind::parse("NeoForge"), Some(LoaderKind::NeoForge));
        // Vanilla und der BuildMC-Client sind keine Loader.
        assert_eq!(LoaderKind::parse("vanilla"), None);
        assert_eq!(LoaderKind::parse("buildmc"), None);
    }

    #[test]
    fn nur_fabric_und_quilt_haben_eine_meta_api() {
        assert!(LoaderKind::Fabric.meta_base().is_some());
        assert!(LoaderKind::Quilt.meta_base().is_some());
        assert!(LoaderKind::NeoForge.meta_base().is_none());
        assert!(LoaderKind::Forge.meta_base().is_none());
    }

    #[test]
    fn unsupported_erklaert_den_grund() {
        let text = unsupported(LoaderKind::NeoForge).to_string();
        assert!(text.contains("NeoForge"));
        assert!(text.contains("Installer"), "der Grund muss genannt werden");
    }
}
