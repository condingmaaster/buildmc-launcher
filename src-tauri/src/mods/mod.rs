//! Modverwaltung: auflisten, installieren, entfernen, umschalten.

pub mod jar;
pub mod meta;
pub mod plan;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::Serialize;

use crate::download::{self, Item, Sink};
use crate::error::{Error, Result};
use crate::{modrinth, paths};

pub use meta::{is_enabled, toggled_name, ModMeta};

/// Eine installierte Mod, wie die Oberflaeche sie sieht.
#[derive(Debug, Clone, Serialize)]
pub struct InstalledMod {
    pub filename: String,
    pub name: String,
    pub version: String,
    pub enabled: bool,
    pub provider: Option<String>,
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub icon_url: Option<String>,
    /// Neuere passende Fassung, sofern gesucht wurde.
    pub update_available: Option<String>,
    pub depends_on: Vec<String>,
    pub size_bytes: u64,
}

pub(crate) fn mods_dir(instance_id: &str) -> Result<PathBuf> {
    let dir = paths::instance_game_dir(instance_id)?.join("mods");
    paths::ensure_dir(&dir)?;
    Ok(dir)
}

/// Alle Dateien im Mod-Ordner, mit dem was wir ueber sie wissen.
///
/// Zu jeder Datei wird zuerst die Ablage befragt; fehlt dort ein Eintrag --
/// weil die Datei von Hand hineingelegt wurde --, wird die Mod selbst
/// gelesen.
pub fn list(instance_id: &str) -> Result<Vec<InstalledMod>> {
    let dir = mods_dir(instance_id)?;
    let known = meta::load(instance_id);
    let mut out = Vec::new();

    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let filename = entry.file_name().to_string_lossy().into_owned();
        if !filename.ends_with(".jar") && !filename.ends_with(".jar.disabled") {
            continue;
        }

        let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
        let enabled = meta::is_enabled(&filename);

        // Die Ablage kann den Eintrag unter beiden Namen fuehren.
        let record = known
            .get(&filename)
            .or_else(|| known.get(&meta::toggled_name(&filename)));

        let (name, version, provider, project_id, version_id, icon_url, depends_on) =
            match record {
                Some(m) => {
                    // Titel aus dem Portal, Fassung aus der Datei: die
                    // Fassungsnummer des Portals ist oft eine andere als
                    // die, die die Mod selbst nennt.
                    let from_jar = jar::read(&path);
                    (
                        if m.title.is_empty() { from_jar.name } else { m.title.clone() },
                        from_jar.version,
                        Some(m.provider.clone()),
                        Some(m.project_id.clone()),
                        Some(m.version_id.clone()),
                        m.icon_url.clone(),
                        m.depends_on.clone(),
                    )
                }
                None => {
                    let from_jar = jar::read(&path);
                    (from_jar.name, from_jar.version, None, None, None, None, Vec::new())
                }
            };

        out.push(InstalledMod {
            filename,
            name,
            version,
            enabled,
            provider,
            project_id,
            version_id,
            icon_url,
            update_available: None,
            depends_on,
            size_bytes,
        });
    }

    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

/// Holt Symbole und Titel fuer alle Eintraege nach, denen sie fehlen.
///
/// Ein Sammelabruf statt eines Aufrufs je Mod -- bei hundert Eintraegen
/// waere das sonst ein Sturm von Anfragen.
pub async fn enrich(instance_id: &str, mods: &mut [InstalledMod]) -> Result<()> {
    let missing: Vec<String> = mods
        .iter()
        .filter(|m| m.icon_url.is_none())
        .filter_map(|m| m.project_id.clone())
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let found = modrinth::projects(&missing).await.unwrap_or_default();
    let mut store = meta::load(instance_id);

    for m in mods.iter_mut() {
        let Some(id) = &m.project_id else { continue };
        let Some((title, icon)) = found.get(id) else { continue };

        m.icon_url = icon.clone();
        if m.name.is_empty() {
            m.name = title.clone();
        }

        // Gleich mitschreiben, damit der naechste Aufruf ohne Netz auskommt.
        if let Some(record) = store.get_mut(&m.filename) {
            record.icon_url = icon.clone();
            if record.title.is_empty() {
                record.title = title.clone();
            }
        }
    }

    let _ = meta::save(instance_id, &store);
    Ok(())
}

/// Installiert eine Fassung in eine Instanz.
///
/// `provider` fehlt bei alten Aufrufen -- dann gilt Modrinth, wie es vor
/// CurseForge immer der Fall war.
pub async fn install(
    instance_id: &str,
    version_id: &str,
    provider: Option<&str>,
    sink: Arc<dyn Sink>,
) -> Result<InstalledMod> {
    let instance = crate::instance::load(instance_id)?;
    let von_curseforge = provider == Some("curseforge");
    let version = if von_curseforge {
        crate::curseforge::version(version_id).await?
    } else {
        modrinth::version(version_id).await?
    };
    plan::ensure_fit(&version, &instance.mc_version, &instance.loader)?;
    let dir = mods_dir(instance_id)?;
    let target = dir.join(&version.filename);

    download::fetch_all(
        vec![Item {
            url: version.url.clone(),
            target: target.clone(),
            sha1: version.sha1.clone(),
            size: version.size_bytes,
        }],
        1,
        true,
        "Mod wird geladen",
        "mod",
        sink,
    )
    .await?;

    // Titel und Symbol gleich mitnehmen, damit die Liste sofort vollstaendig ist.
    // CurseForge hat keinen Sammelabfrage-Endpunkt wie Modrinth -- bei
    // genau einem Projekt macht das ohnehin keinen Unterschied.
    let (title, icon_url) = if von_curseforge {
        crate::curseforge::project(&version.project_id)
            .await
            .unwrap_or((version.name.clone(), None))
    } else {
        modrinth::projects(&[version.project_id.clone()])
            .await
            .unwrap_or_default()
            .get(&version.project_id)
            .cloned()
            .unwrap_or((version.name.clone(), None))
    };

    meta::set(
        instance_id,
        &version.filename,
        ModMeta {
            provider: version.provider.clone(),
            project_id: version.project_id.clone(),
            version_id: version.version_id.clone(),
            title: title.clone(),
            icon_url: icon_url.clone(),
            depends_on: Vec::new(),
            installed_at: crate::instance::now(),
        },
    )?;

    let from_jar = jar::read(&target);
    Ok(InstalledMod {
        filename: version.filename,
        name: title,
        version: from_jar.version,
        enabled: true,
        provider: Some(version.provider),
        project_id: Some(version.project_id),
        version_id: Some(version.version_id),
        icon_url,
        update_available: None,
        depends_on: Vec::new(),
        size_bytes: version.size_bytes,
    })
}

/// Legt eine vorhandene Datei in die Instanz.
pub fn add_local(instance_id: &str, source: &Path) -> Result<InstalledMod> {
    if !source.is_file() {
        return Err(Error::not_found(format!("{} ist keine Datei", source.display())));
    }
    let name = source
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| Error::other("Dateiname nicht lesbar"))?;

    if !name.ends_with(".jar") {
        return Err(Error::other("Nur .jar-Dateien können als Mod hinzugefügt werden"));
    }

    let target = mods_dir(instance_id)?.join(&name);
    std::fs::copy(source, &target)?;

    let info = jar::read(&target);
    Ok(InstalledMod {
        filename: name,
        name: info.name,
        version: info.version,
        enabled: true,
        provider: None,
        project_id: None,
        version_id: None,
        icon_url: None,
        update_available: None,
        depends_on: Vec::new(),
        size_bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
    })
}

pub fn remove(instance_id: &str, filename: &str) -> Result<()> {
    let dir = mods_dir(instance_id)?;
    let path = paths::safe_join(&dir, filename)?;
    if path.exists() {
        std::fs::remove_file(&path)?;
    } else {
        // Vielleicht liegt sie im anderen Zustand vor.
        let alt = dir.join(meta::toggled_name(filename));
        if alt.exists() {
            std::fs::remove_file(alt)?;
        }
    }
    meta::remove(instance_id, filename)
}

/// Schaltet eine Mod an oder aus, indem die Datei umbenannt wird.
pub fn toggle(instance_id: &str, filename: &str) -> Result<String> {
    let dir = mods_dir(instance_id)?;
    let from = paths::safe_join(&dir, filename)?;
    if !from.exists() {
        return Err(Error::not_found(format!("{filename} liegt nicht im Mod-Ordner")));
    }

    let new_name = meta::toggled_name(filename);
    let to = dir.join(&new_name);
    if to.exists() {
        return Err(Error::other(format!(
            "{new_name} ist bereits vorhanden — die Mod liegt doppelt vor"
        )));
    }

    std::fs::rename(&from, &to)?;
    meta::rename(instance_id, filename, &new_name)?;
    Ok(new_name)
}

/// Sucht zu jeder installierten Mod eine neuere passende Fassung.
pub async fn check_updates(
    instance_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<Vec<InstalledMod>> {
    let mut mods = list(instance_id)?;

    for m in mods.iter_mut() {
        let (Some(project_id), Some(current)) = (&m.project_id, &m.version_id) else {
            // Von Hand hinzugefuegte Mods lassen sich nicht zuordnen.
            continue;
        };

        let available = modrinth::versions(project_id, Some(game_version), Some(loader))
            .await
            .unwrap_or_default();

        // Die neueste endgueltige Fassung, sofern sie eine andere ist.
        if let Some(newest) = available
            .iter()
            .find(|v| v.release_type == "release")
            .or_else(|| available.first())
        {
            if &newest.version_id != current {
                m.update_available = Some(newest.version_number.clone());
            }
        }
    }

    Ok(mods)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Eigenes Verzeichnis je Test.
    ///
    /// Rust fuehrt Tests nebenlaeufig aus. Teilen sie sich eine Instanz,
    /// raeumt der eine weg, was der andere gerade benutzt -- der Test
    /// schlaegt dann mal fehl und mal nicht. Genau das ist hier passiert.
    fn temp_instance(marke: &str) -> String {
        use std::sync::atomic::{AtomicU32, Ordering};
        static ZAEHLER: AtomicU32 = AtomicU32::new(0);
        let id = format!(
            "test-mods-{}-{marke}-{}",
            std::process::id(),
            ZAEHLER.fetch_add(1, Ordering::Relaxed)
        );
        let dir = paths::instance_game_dir(&id).unwrap().join("mods");
        std::fs::create_dir_all(&dir).unwrap();
        id
    }

    fn cleanup(id: &str) {
        let _ = std::fs::remove_dir_all(paths::instance_dir(id).unwrap());
    }

    #[test]
    fn umschalten_benennt_um_und_zurueck() {
        let id = temp_instance("umschalten");
        let dir = mods_dir(&id).unwrap();
        std::fs::write(dir.join("sodium.jar"), b"x").unwrap();

        let disabled = toggle(&id, "sodium.jar").unwrap();
        assert_eq!(disabled, "sodium.jar.disabled");
        assert!(dir.join("sodium.jar.disabled").exists());
        assert!(!dir.join("sodium.jar").exists());

        let enabled = toggle(&id, "sodium.jar.disabled").unwrap();
        assert_eq!(enabled, "sodium.jar");
        assert!(dir.join("sodium.jar").exists());

        cleanup(&id);
    }

    #[test]
    fn liste_findet_beide_zustaende() {
        let id = temp_instance("liste");
        let dir = mods_dir(&id).unwrap();
        std::fs::write(dir.join("aaa-1.0.jar"), b"x").unwrap();
        std::fs::write(dir.join("bbb-2.0.jar.disabled"), b"x").unwrap();
        // Keine Mod -- muss ausgelassen werden.
        std::fs::write(dir.join("notizen.txt"), b"x").unwrap();

        let mods = list(&id).unwrap();
        assert_eq!(mods.len(), 2, "die Textdatei darf nicht mitgezählt werden");
        assert!(mods.iter().any(|m| m.enabled));
        assert!(mods.iter().any(|m| !m.enabled));

        cleanup(&id);
    }

    #[test]
    fn entfernen_raeumt_auch_den_abgeschalteten_zustand() {
        let id = temp_instance("entfernen");
        let dir = mods_dir(&id).unwrap();
        std::fs::write(dir.join("weg.jar.disabled"), b"x").unwrap();

        remove(&id, "weg.jar").unwrap();
        assert!(!dir.join("weg.jar.disabled").exists());

        cleanup(&id);
    }

    #[test]
    fn nur_jar_dateien_werden_angenommen() {
        let id = temp_instance("nur-jar");
        // Auch die Quelldatei eindeutig benennen.
        let quelle = std::env::temp_dir().join(format!("keine-mod-{}.txt", std::process::id()));
        std::fs::write(&quelle, b"x").unwrap();

        assert!(add_local(&id, &quelle).is_err());

        std::fs::remove_file(&quelle).ok();
        cleanup(&id);
    }
}
