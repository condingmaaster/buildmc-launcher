//! Nach neuen Fassungen sehen.
//!
//! Die Veroeffentlichungen liegen als GitHub-Releases. Geprueft wird die
//! Versionsnummer, nicht das Datum -- ein spaeter nachgereichtes Paket
//! einer aelteren Fassung darf keine Aktualisierung ausloesen.
//!
//! Heruntergeladen wird nur; **eingespielt wird nicht automatisch.** Eine
//! laufende Anwendung durch sich selbst zu ersetzen, ist je nach System
//! heikel, und ein Paketverwalter will ohnehin gefragt werden. Der Launcher
//! legt die Datei ab und ueberlaesst den letzten Schritt dem Nutzer.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::{http, paths};

const RELEASES_URL: &str =
    "https://api.github.com/repos/jonagotthardt/jona/releases?per_page=20";

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
    #[serde(default)]
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateInfo {
    pub version: String,
    pub name: String,
    pub notes: String,
    pub url: String,
    pub filename: String,
    pub size: u64,
    pub prerelease: bool,
}

/// Zerlegt eine Versionsangabe in Zahlen.
///
/// Vertraegt `v0.2.0`, `Beta-Release-0.1.3` und `0.2` gleichermassen --
/// die Schreibweise der Marken war in der Vergangenheit uneinheitlich.
pub fn parse_version(text: &str) -> Vec<u32> {
    let digits: String = text
        .chars()
        .map(|c| if c.is_ascii_digit() || c == '.' { c } else { ' ' })
        .collect();

    digits
        .split_whitespace()
        .find(|part| part.contains('.') || part.chars().all(|c| c.is_ascii_digit()))
        .map(|part| {
            part.split('.')
                .filter(|s| !s.is_empty())
                .filter_map(|s| s.parse().ok())
                .collect()
        })
        .unwrap_or_default()
}

/// Ist `candidate` neuer als `current`?
pub fn is_newer(candidate: &str, current: &str) -> bool {
    let a = parse_version(candidate);
    let b = parse_version(current);
    if a.is_empty() {
        return false;
    }
    // Fehlende Stellen gelten als 0, damit 0.2 und 0.2.0 gleich sind.
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Welches Paket zum laufenden System passt.
pub fn matching_asset<'a>(assets: &'a [ReleaseAsset]) -> Option<&'a ReleaseAsset> {
    let wanted: &[&str] = if cfg!(target_os = "windows") {
        &[".msi", ".exe"]
    } else if cfg!(target_os = "macos") {
        &[".dmg", ".app.tar.gz"]
    } else {
        // Auf Linux ist das .deb die uebliche Form; AppImage als Ausweichweg.
        &[".deb", ".appimage"]
    };

    for suffix in wanted {
        if let Some(found) = assets
            .iter()
            .find(|a| a.name.to_ascii_lowercase().ends_with(suffix))
        {
            return Some(found);
        }
    }
    None
}

/// Sieht nach, ob es etwas Neueres gibt.
///
/// Vorabfassungen werden uebergangen, sofern nicht ausdruecklich gewuenscht.
pub async fn check(current: &str, allow_prerelease: bool) -> Result<Option<UpdateInfo>> {
    let releases: Vec<Release> = http::get_json(RELEASES_URL).await?;

    let newest = releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter(|r| allow_prerelease || !r.prerelease)
        .filter(|r| is_newer(&r.tag_name, current))
        .max_by(|a, b| {
            parse_version(&a.tag_name).cmp(&parse_version(&b.tag_name))
        });

    let Some(release) = newest else { return Ok(None) };
    let Some(asset) = matching_asset(&release.assets) else {
        // Neuere Fassung vorhanden, aber nicht fuer dieses System.
        return Ok(None);
    };

    Ok(Some(UpdateInfo {
        version: release.tag_name.clone(),
        name: if release.name.is_empty() { release.tag_name } else { release.name },
        notes: release.body,
        url: asset.browser_download_url.clone(),
        filename: asset.name.clone(),
        size: asset.size,
        prerelease: release.prerelease,
    }))
}

/// Laedt das Paket herunter und legt es ab.
///
/// Gibt den Pfad zurueck. Eingespielt wird bewusst nicht -- siehe oben.
pub async fn download(url: &str, filename: &str) -> Result<String> {
    // Keinen Pfad aus dem Dateinamen zulassen: er kommt von aussen.
    if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
        return Err(Error::other("Ungültiger Dateiname im Veröffentlichungspaket"));
    }

    let dir = paths::root()?.join("updates");
    paths::ensure_dir(&dir)?;
    let target = dir.join(filename);

    let bytes = http::get_bytes(url).await?;
    std::fs::write(&target, &bytes)?;
    Ok(target.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> ReleaseAsset {
        ReleaseAsset {
            name: name.into(),
            browser_download_url: format!("https://example.org/{name}"),
            size: 1,
        }
    }

    #[test]
    fn versionsnummern_aus_verschiedenen_schreibweisen() {
        assert_eq!(parse_version("v0.2.0"), vec![0, 2, 0]);
        assert_eq!(parse_version("Beta-Release-0.1.3"), vec![0, 1, 3]);
        assert_eq!(parse_version("0.2"), vec![0, 2]);
        assert!(parse_version("ohne Zahlen").is_empty());
    }

    #[test]
    fn neuere_fassung_wird_erkannt() {
        assert!(is_newer("0.2.0", "0.1.3"));
        assert!(is_newer("Beta-Release-0.1.3", "0.1.2"));
        assert!(is_newer("1.0.0", "0.9.9"));
    }

    #[test]
    fn gleiche_oder_aeltere_loest_nichts_aus() {
        assert!(!is_newer("0.1.3", "0.1.3"));
        assert!(!is_newer("0.1.2", "0.1.3"));
        // Fehlende Stellen gelten als null.
        assert!(!is_newer("0.2", "0.2.0"));
        assert!(!is_newer("kaputt", "0.1.0"));
    }

    #[test]
    fn passendes_paket_je_system() {
        let assets = vec![
            asset("LINUX_BuildMC.Launcher_0.2.0_amd64.deb"),
            asset("WINDOWS_BuildMC.Launcher_0.2.0_x64_en-US.msi"),
            asset("LINUX_BuildMC-0.2.0.AppImage"),
        ];
        let found = matching_asset(&assets).expect("eines muss passen");
        if cfg!(target_os = "windows") {
            assert!(found.name.ends_with(".msi"));
        } else if cfg!(target_os = "linux") {
            // Das .deb wird dem AppImage vorgezogen.
            assert!(found.name.ends_with(".deb"), "{}", found.name);
        }
    }

    #[test]
    fn ohne_passendes_paket_kein_treffer() {
        assert!(matching_asset(&[asset("quelltext.tar.gz")]).is_none());
        assert!(matching_asset(&[]).is_none());
    }

    #[test]
    fn dateiname_mit_pfad_wird_abgewiesen() {
        let r = tokio::runtime::Builder::new_current_thread().build().unwrap();
        assert!(r.block_on(download("https://x", "../../boese.deb")).is_err());
        assert!(r.block_on(download("https://x", "a/b.deb")).is_err());
    }
}

#[cfg(test)]
mod versions_tests {
    /// Die Versionsnummer steht an drei Stellen, und sie müssen
    /// übereinstimmen.
    ///
    /// Tauri baut das Paket nach `tauri.conf.json`, die Update-Prüfung
    /// vergleicht aber gegen `CARGO_PKG_VERSION` aus der `Cargo.toml`.
    /// Laufen die auseinander, hält sich der Launcher für älter, als er
    /// ist, und bietet dauerhaft eine Aktualisierung auf eine Fassung an,
    /// die bereits installiert ist. Genau das ist bei 0.2.0/0.2.1 passiert.
    #[test]
    fn versionsnummern_stimmen_ueberein() {
        let conf: serde_json::Value = serde_json::from_str(
            include_str!("../../tauri.conf.json"),
        )
        .expect("tauri.conf.json muss lesbar sein");

        let im_bundle = conf["version"].as_str().expect("version fehlt");
        assert_eq!(
            im_bundle,
            env!("CARGO_PKG_VERSION"),
            "tauri.conf.json und Cargo.toml müssen dieselbe Version nennen"
        );
    }

    /// Linux erlaubt kein Leerzeichen im Paketnamen -- die Datei müsste
    /// sonst vor jeder Installation von Hand umbenannt werden.
    #[test]
    fn produktname_ohne_leerzeichen() {
        let conf: serde_json::Value = serde_json::from_str(
            include_str!("../../tauri.conf.json"),
        )
        .unwrap();

        let name = conf["productName"].as_str().expect("productName fehlt");
        assert!(!name.contains(' '), "productName enthält ein Leerzeichen: {name}");
    }
}
