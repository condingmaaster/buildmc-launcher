//! Java selbst besorgen.
//!
//! Wer Minecraft 1.21 starten will, braucht Java 21. Ist keines da, endet
//! der Start mit einer Meldung, die den Nutzer allein laesst -- er soll
//! sich irgendwo ein JDK suchen. Das ist unnoetig: Adoptium stellt fuer
//! jedes System fertige Pakete bereit, samt Pruefsumme.
//!
//! Abgelegt wird unter `jdks/`, damit die eigene Installation von einer
//! systemweiten unterscheidbar bleibt.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::error::{Error, Result};
use crate::{http, paths};

const API: &str = "https://api.adoptium.net/v3/assets/latest";

#[derive(Debug, Deserialize)]
struct Asset {
    release_name: String,
    binary: Binary,
}

#[derive(Debug, Deserialize)]
struct Binary {
    package: Package,
}

#[derive(Debug, Deserialize)]
struct Package {
    link: String,
    #[serde(default)]
    size: u64,
    #[serde(default)]
    checksum: String,
}

/// Wie Adoptium das laufende System nennt.
pub fn platform() -> (&'static str, &'static str) {
    let os = match std::env::consts::OS {
        "windows" => "windows",
        "macos" => "mac",
        _ => "linux",
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "aarch64",
        "x86" => "x86",
        other => other,
    };
    (os, arch)
}

/// Sucht das passende Paket.
pub async fn find(major: u32) -> Result<(String, String, u64, String)> {
    let (os, arch) = platform();
    let url = format!(
        "{API}/{major}/hotspot?architecture={arch}&image_type=jdk&os={os}&vendor=eclipse"
    );

    let assets: Vec<Asset> = http::get_json(&url).await?;
    let asset = assets.into_iter().next().ok_or_else(|| {
        Error::not_found(format!(
            "Für Java {major} gibt es bei Adoptium kein Paket für {os}/{arch}"
        ))
    })?;

    Ok((
        asset.release_name,
        asset.binary.package.link,
        asset.binary.package.size,
        asset.binary.package.checksum,
    ))
}

/// Laedt Java herunter und packt es aus.
///
/// Die Pruefsumme wird verglichen, bevor ausgepackt wird -- ein
/// abgebrochener Download ergaebe sonst ein halbes JDK, das erst beim
/// Start auffaellt.
pub async fn install(major: u32) -> Result<super::JavaInfo> {
    let (release, link, size, checksum) = find(major).await?;

    let root = paths::jdks()?;
    paths::ensure_dir(&root)?;
    let target = root.join(&release);

    // Schon da? Dann nur pruefen und melden.
    if target.exists() {
        if let Ok(info) = probe_dir(&target) {
            return Ok(info);
        }
        std::fs::remove_dir_all(&target)?;
    }

    let archive = root.join(format!("{release}.download"));
    let bytes = http::get_bytes(&link).await?;

    if size > 0 && bytes.len() as u64 != size {
        return Err(Error::other(format!(
            "Der Download ist unvollständig ({} statt {size} Bytes)",
            bytes.len()
        )));
    }
    if !checksum.is_empty() {
        let got = hex::encode(Sha256::digest(&bytes));
        if got != checksum {
            return Err(Error::Checksum {
                datei: release.clone(),
                erwartet: checksum,
                erhalten: got,
            });
        }
    }

    std::fs::write(&archive, &bytes)?;
    let result = extract(&archive, &target, link.ends_with(".zip"));
    let _ = std::fs::remove_file(&archive);
    result?;

    probe_dir(&target)
}

fn extract(archive: &Path, target: &Path, is_zip: bool) -> Result<()> {
    paths::ensure_dir(target)?;
    let file = std::fs::File::open(archive)?;

    if is_zip {
        let mut zip = zip::ZipArchive::new(file)?;
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
            // Adoptium packt alles in einen Ordner mit dem Versionsnamen --
            // die Ebene wird uebersprungen, damit `bin/` direkt liegt.
            let Some(stripped) = strip_first(&name) else { continue };
            let out = target.join(stripped);
            if entry.is_dir() {
                paths::ensure_dir(&out)?;
                continue;
            }
            paths::ensure_parent(&out)?;
            let mut sink = std::fs::File::create(&out)?;
            std::io::copy(&mut entry, &mut sink)?;
        }
        return Ok(());
    }

    let decoder = flate2::read::GzDecoder::new(file);
    let mut tar = tar::Archive::new(decoder);

    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let Some(stripped) = strip_first(&path) else { continue };
        let out = target.join(stripped);

        if entry.header().entry_type().is_dir() {
            paths::ensure_dir(&out)?;
            continue;
        }
        paths::ensure_parent(&out)?;

        let mut sink = std::fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut sink)?;

        // Ausfuehrbarkeit erhalten -- ohne sie startet `java` nicht.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if let Ok(mode) = entry.header().mode() {
                let _ = std::fs::set_permissions(&out, std::fs::Permissions::from_mode(mode));
            }
        }
    }
    Ok(())
}

/// Entfernt die oberste Ebene eines Pfades.
fn strip_first(path: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    parts.next()?;
    let rest: PathBuf = parts.collect();
    (!rest.as_os_str().is_empty()).then_some(rest)
}

/// Prueft eine ausgepackte Installation.
fn probe_dir(dir: &Path) -> Result<super::JavaInfo> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    // macOS schachtelt eine Ebene tiefer.
    for candidate in [
        dir.join("bin").join(exe),
        dir.join("Contents").join("Home").join("bin").join(exe),
    ] {
        if candidate.exists() {
            return super::probe(&candidate);
        }
    }
    Err(Error::not_found("Im entpackten Paket ist kein java zu finden"))
}

/// Entfernt eine selbst installierte Fassung.
pub fn uninstall(release: &str) -> Result<()> {
    let dir = paths::jdks()?.join(release);
    // Nur innerhalb des eigenen Verzeichnisses loeschen.
    if !dir.starts_with(paths::jdks()?) || release.contains("..") {
        return Err(Error::other("Ungültiger Name"));
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn systemnamen_wie_adoptium_sie_kennt() {
        let (os, arch) = platform();
        assert!(["linux", "windows", "mac"].contains(&os), "{os}");
        assert!(["x64", "aarch64", "x86"].contains(&arch), "{arch}");
    }

    #[test]
    fn oberste_ebene_wird_entfernt() {
        assert_eq!(
            strip_first(Path::new("jdk-21.0.12+1/bin/java")),
            Some(PathBuf::from("bin/java"))
        );
        // Die oberste Ebene allein ergibt nichts.
        assert_eq!(strip_first(Path::new("jdk-21")), None);
    }

    #[test]
    fn ausbruch_aus_dem_verzeichnis_wird_verhindert() {
        assert!(uninstall("../../etc").is_err());
    }
}
