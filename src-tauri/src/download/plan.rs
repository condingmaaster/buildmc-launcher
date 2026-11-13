//! Aus einer Versions-JSON die konkrete Dateiliste ableiten.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::Deserialize;

use crate::download::Item;
use crate::error::{Error, Result};
use crate::manifest::rules::RuleContext;
use crate::manifest::{Library, VersionInfo, ASSET_BASE};
use crate::{http, paths};

/// Der Asset-Index: Name -> Hash und Groesse.
#[derive(Debug, Clone, Deserialize)]
pub struct AssetIndex {
    pub objects: HashMap<String, AssetObject>,
    /// Alte Versionen legen die Dateien unter ihrem echten Namen ab.
    #[serde(default)]
    pub map_to_resources: bool,
    #[serde(default)]
    pub r#virtual: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    #[serde(default)]
    pub size: u64,
}

impl AssetObject {
    /// `<hash[0..2]>/<hash>` -- so liegen die Objekte auf dem Server
    /// und lokal unter `assets/objects/`.
    ///
    /// `hash[0..2]` wuerde bei einem zu kurzen Hash abstuerzen (Byte-Index
    /// ausserhalb der Zeichenkette) -- ein beschaedigter oder manipulierter
    /// Asset-Index darf nicht den ganzen Launcher mitreissen, nur weil ein
    /// einzelnes Objekt eine unerwartete Kennung hat.
    pub fn rel_path(&self) -> PathBuf {
        let prefix = self.hash.get(0..2).unwrap_or(self.hash.as_str());
        PathBuf::from(prefix).join(&self.hash)
    }
}

/// Client-Jar der Version.
pub fn client_jar(info: &VersionInfo) -> Result<Option<Item>> {
    let Some(downloads) = &info.downloads else {
        return Ok(None);
    };
    let Some(client) = &downloads.client else {
        return Ok(None);
    };
    Ok(Some(Item {
        url: client.url.clone(),
        target: paths::version_jar(&info.id)?,
        sha1: client.sha1.clone(),
        size: client.size,
    }))
}

/// Alle fuer dieses System geltenden Bibliotheken, inklusive der Natives.
///
/// Liefert Paare aus Eintrag und dem Hinweis, ob es sich um eine native
/// Bibliothek handelt -- die muessen spaeter entpackt statt in den
/// Klassenpfad gelegt zu werden.
pub fn libraries(info: &VersionInfo, ctx: &RuleContext) -> Result<Vec<(Item, bool)>> {
    let lib_root = paths::libraries()?;
    let mut out = Vec::new();

    for lib in &info.libraries {
        if !ctx.allows(&lib.rules) {
            continue;
        }

        // Gewoehnliches Artefakt
        if let Some(item) = artifact_item(lib, &lib_root)? {
            out.push((item, false));
        }

        // Natives: die alte Form nennt einen Klassifizierer je System.
        if let Some(classifier) = native_classifier(lib, ctx) {
            if let Some(downloads) = &lib.downloads {
                if let Some(info) = downloads.classifiers.get(&classifier) {
                    let rel = info
                        .path
                        .clone()
                        .map(PathBuf::from)
                        .unwrap_or(paths::maven_path(&format!("{}:{classifier}", lib.name))?);
                    out.push((
                        Item {
                            url: info.url.clone(),
                            target: lib_root.join(rel),
                            sha1: info.sha1.clone(),
                            size: info.size,
                        },
                        true,
                    ));
                }
            }
        }
    }

    Ok(out)
}

fn artifact_item(lib: &Library, lib_root: &std::path::Path) -> Result<Option<Item>> {
    // Form 1: vollstaendige Angaben im Manifest.
    if let Some(downloads) = &lib.downloads {
        if let Some(a) = &downloads.artifact {
            let rel = match &a.path {
                Some(p) => PathBuf::from(p),
                None => paths::maven_path(&lib.name)?,
            };
            return Ok(Some(Item {
                url: a.url.clone(),
                target: lib_root.join(rel),
                sha1: a.sha1.clone(),
                size: a.size,
            }));
        }
    }

    // Form 2: nur Koordinaten und eine Maven-Basis (Fabric, NeoForge).
    if let Some(base) = &lib.url {
        let rel = paths::maven_path(&lib.name)?;
        let url = format!(
            "{}/{}",
            base.trim_end_matches('/'),
            rel.to_string_lossy().replace('\\', "/")
        );
        return Ok(Some(Item {
            url,
            target: lib_root.join(rel),
            sha1: None,
            size: 0,
        }));
    }

    Ok(None)
}

/// Der Klassifizierer fuer native Bibliotheken, z. B. `natives-linux`.
fn native_classifier(lib: &Library, ctx: &RuleContext) -> Option<String> {
    let raw = lib.natives.get(ctx.os_name)?;
    // Mojang setzt hier `${arch}` ein: 32 oder 64.
    let arch = if ctx.os_arch == "x86" { "32" } else { "64" };
    Some(raw.replace("${arch}", arch))
}

/// Laedt den Asset-Index und leitet daraus die Objektliste ab.
pub async fn assets(info: &VersionInfo) -> Result<(Vec<Item>, AssetIndex)> {
    let Some(reference) = &info.asset_index else {
        return Err(Error::other(format!(
            "Version {} nennt keinen Asset-Index",
            info.id
        )));
    };

    let index_path = paths::assets()?
        .join("indexes")
        .join(format!("{}.json", reference.id));

    // Blockierendes Lesen/Schreiben nie direkt in dieser async fn -- der
    // Index kann bei modernen Versionen 10-50 MB gross sein (virtuelle
    // Legacy-Assets), und ein blockierender Zugriff dieser Groesse auf
    // einem der wenigen Tokio-Arbeitsthreads kann die ganze Tauri-
    // Oberflaeche einfrieren lassen ("reagiert nicht") -- derselbe Fehler,
    // der schon einmal bei der Sicherheitspruefung auftrat.
    let raw = if index_path.exists() {
        let p = index_path.clone();
        tokio::task::spawn_blocking(move || std::fs::read(&p))
            .await
            .map_err(|e| Error::other(format!("Asset-Index nicht lesbar: {e}")))??
    } else {
        let bytes = http::get_bytes(&reference.url).await?;
        paths::ensure_parent(&index_path)?;
        let p = index_path.clone();
        let zu_schreiben = bytes.clone();
        tokio::task::spawn_blocking(move || std::fs::write(&p, &zu_schreiben))
            .await
            .map_err(|e| Error::other(format!("Asset-Index nicht schreibbar: {e}")))??;
        bytes
    };

    let index: AssetIndex = serde_json::from_slice(&raw)?;
    let objects_root = paths::assets()?.join("objects");

    let items = index
        .objects
        .values()
        .map(|o| Item {
            url: format!("{ASSET_BASE}/{}/{}", &o.hash[0..2], o.hash),
            target: objects_root.join(o.rel_path()),
            sha1: Some(o.hash.clone()),
            size: o.size,
        })
        .collect();

    Ok((items, index))
}

/// Entpackt die nativen Bibliotheken in das Verzeichnis, das spaeter als
/// `java.library.path` uebergeben wird.
///
/// Signaturdateien aus dem Jar-Manifest werden ausgelassen; sie gehoeren
/// nicht zu den geladenen Bibliotheken und wuerden nur stoeren.
pub fn extract_natives(archives: &[PathBuf], target: &std::path::Path) -> Result<()> {
    paths::ensure_dir(target)?;

    for archive in archives {
        let file = std::fs::File::open(archive)?;
        let mut zip = zip::ZipArchive::new(file)?;

        for i in 0..zip.len() {
            let mut entry = zip.by_index(i)?;
            if entry.is_dir() {
                continue;
            }
            let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else {
                continue;
            };
            let as_str = name.to_string_lossy();
            if as_str.starts_with("META-INF/") || as_str.contains("__MACOSX") {
                continue;
            }

            // Flach ausrollen: Java sucht die Dateien direkt im Pfad.
            let Some(file_name) = name.file_name() else { continue };
            let out = target.join(file_name);
            if out.exists() {
                continue;
            }

            // Erst in eine Nebendatei, dann umbenennen -- ein abgebrochener
            // Vorgang (Absturz, erzwungenes Beenden) darf keine halbe
            // Datei hinterlassen, die `out.exists()` beim naechsten Start
            // faelschlich als vollstaendig durchgehen liesse und die
            // Instanz dauerhaft mit einer kaputten nativen Bibliothek
            // startunfaehig machen wuerde.
            let tmp = out.with_extension("part");
            let mut sink = std::fs::File::create(&tmp)?;
            std::io::copy(&mut entry, &mut sink)?;
            drop(sink);
            std::fs::rename(&tmp, &out)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::AssetObject;

    #[test]
    fn normaler_hash_ergibt_den_ueblichen_pfad() {
        let o = AssetObject { hash: "abcdef0123456789".into(), size: 0 };
        assert_eq!(o.rel_path(), std::path::Path::new("ab/abcdef0123456789"));
    }

    #[test]
    fn zu_kurzer_hash_stuerzt_nicht_ab() {
        // Ein beschaedigter Asset-Index darf hoechstens ein einzelnes
        // Objekt betreffen, nie den ganzen Launcher mitreissen.
        assert_eq!(
            AssetObject { hash: "a".into(), size: 0 }.rel_path(),
            std::path::Path::new("a/a")
        );
        assert_eq!(
            AssetObject { hash: String::new(), size: 0 }.rel_path(),
            std::path::Path::new("")
        );
    }
}
