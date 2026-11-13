//! Sicherungen einer Instanz.
//!
//! Gesichert wird in ein Zip-Archiv neben der Instanz. Welten sind
//! abwaehlbar, weil sie den Loewenanteil ausmachen: eine gewachsene Welt
//! wiegt leicht mehr als alle Mods zusammen. Wer nur vor einem Mod-Update
//! sichert, braucht sie nicht -- wer vor einem Versionswechsel sichert,
//! sehr wohl.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub id: String,
    pub instance_id: String,
    pub label: String,
    pub created_at: String,
    pub size_bytes: u64,
    pub includes_worlds: bool,
    /// Vom Launcher selbst angelegt, nicht vom Nutzer.
    pub automatic: bool,
}

/// Was nie mitgesichert wird.
///
/// Protokolle, Zwischenspeicher und entpackte Bibliotheken lassen sich
/// jederzeit neu erzeugen und wuerden die Sicherung vervielfachen.
/// Was nie mitgesichert wird.
///
/// Protokolle, Zwischenspeicher und entpackte Bibliotheken lassen sich
/// jederzeit neu erzeugen. `instance.json` gehoert dem Launcher, nicht
/// dem Spiel -- sie liegt im selben Ordner, weil das Spielverzeichnis der
/// Instanzordner ist.
const SKIP: [&str; 8] = [
    "logs", ".fabric", ".mixin.out", "crash-reports", "downloads", ".bobby",
    "instance.json", "mods.json",
];

fn dir(instance_id: &str) -> Result<std::path::PathBuf> {
    let d = paths::safe_join(&paths::snapshots()?, instance_id)?;
    paths::ensure_dir(&d)?;
    Ok(d)
}

fn archive_path(instance_id: &str, id: &str) -> Result<std::path::PathBuf> {
    if !paths::is_bare_name(id) {
        return Err(Error::other(format!("Ungültiger Name: {id}")));
    }
    Ok(dir(instance_id)?.join(format!("{id}.zip")))
}

fn index_path(instance_id: &str) -> Result<std::path::PathBuf> {
    Ok(dir(instance_id)?.join("index.json"))
}

pub fn list(instance_id: &str) -> Vec<Snapshot> {
    index_path(instance_id)
        .map(|p| crate::store::read_or_default::<Vec<Snapshot>>(&p))
        .unwrap_or_default()
}

fn save_index(instance_id: &str, list: &[Snapshot]) -> Result<()> {
    crate::store::write(&index_path(instance_id)?, &list)
}

/// Legt eine Sicherung an.
pub fn create(
    instance_id: &str,
    label: &str,
    include_worlds: bool,
    automatic: bool,
) -> Result<Snapshot> {
    let game_dir = paths::instance_game_dir(instance_id)?;
    if !game_dir.exists() {
        return Err(Error::not_found("Die Instanz hat noch kein Spielverzeichnis"));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let target = archive_path(instance_id, &id)?;

    let file = std::fs::File::create(&target)?;
    let mut zip = zip::ZipWriter::new(file);
    let options: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    add_dir(&mut zip, &game_dir, &game_dir, include_worlds, &options)?;
    zip.finish()?;

    let snapshot = Snapshot {
        size_bytes: std::fs::metadata(&target).map(|m| m.len()).unwrap_or(0),
        id,
        instance_id: instance_id.to_string(),
        label: if label.trim().is_empty() { "Ohne Namen".into() } else { label.trim().into() },
        created_at: crate::instance::now(),
        includes_worlds: include_worlds,
        automatic,
    };

    let mut all = list(instance_id);
    all.insert(0, snapshot.clone());
    save_index(instance_id, &all)?;
    Ok(snapshot)
}

fn add_dir(
    zip: &mut zip::ZipWriter<std::fs::File>,
    root: &Path,
    current: &Path,
    include_worlds: bool,
    options: &zip::write::FileOptions<'_, ()>,
) -> Result<()> {
    for entry in std::fs::read_dir(current)? {
        let entry = entry?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();

        if SKIP.contains(&name.as_str()) {
            continue;
        }
        if !include_worlds && name == "saves" && path.is_dir() {
            continue;
        }

        let relative = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().replace('\\', "/");

        if path.is_dir() {
            add_dir(zip, root, &path, include_worlds, options)?;
        } else {
            zip.start_file(relative, *options)?;
            let mut source = std::fs::File::open(&path)?;
            std::io::copy(&mut source, zip)?;
        }
    }
    Ok(())
}

/// Spielt eine Sicherung zurueck.
///
/// Vorher wird der aktuelle Stand automatisch gesichert. Ohne das waere
/// ein versehentliches Zuruecksetzen unumkehrbar -- und genau dafuer ist
/// die Funktion ja da.
pub fn restore(instance_id: &str, snapshot_id: &str) -> Result<()> {
    let archive = archive_path(instance_id, snapshot_id)?;
    if !archive.exists() {
        return Err(Error::not_found("Diese Sicherung gibt es nicht mehr"));
    }

    let _ = create(instance_id, "Vor dem Zurücksetzen", true, true);

    let game_dir = paths::instance_game_dir(instance_id)?;
    let file = std::fs::File::open(&archive)?;
    let mut zip = zip::ZipArchive::new(file)?;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        let Some(name) = entry.enclosed_name().map(|p| p.to_path_buf()) else { continue };
        let target = game_dir.join(&name);
        if entry.is_dir() {
            paths::ensure_dir(&target)?;
            continue;
        }
        paths::ensure_parent(&target)?;
        let mut out = std::fs::File::create(&target)?;
        std::io::copy(&mut entry, &mut out)?;
        out.flush()?;
    }

    Ok(())
}

pub fn delete(instance_id: &str, snapshot_id: &str) -> Result<()> {
    let archive = archive_path(instance_id, snapshot_id)?;
    if archive.exists() {
        std::fs::remove_file(archive)?;
    }
    let remaining: Vec<Snapshot> = list(instance_id)
        .into_iter()
        .filter(|s| s.id != snapshot_id)
        .collect();
    save_index(instance_id, &remaining)
}

/// Entfernt die aeltesten automatischen Sicherungen ueber der Grenze.
///
/// Von Hand angelegte bleiben unangetastet -- die hat jemand bewusst
/// gemacht und will sie behalten.
pub fn prune(instance_id: &str, keep: usize) -> Result<usize> {
    let all = list(instance_id);
    let automatic: Vec<&Snapshot> = all.iter().filter(|s| s.automatic).collect();
    if automatic.len() <= keep {
        return Ok(0);
    }

    let to_remove: Vec<String> = automatic
        .into_iter()
        .skip(keep)
        .map(|s| s.id.clone())
        .collect();

    for id in &to_remove {
        delete(instance_id, id)?;
    }
    Ok(to_remove.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(marke: &str) -> String {
        use std::sync::atomic::{AtomicU32, Ordering};
        static Z: AtomicU32 = AtomicU32::new(0);
        let id = format!("snap-{}-{marke}-{}", std::process::id(), Z.fetch_add(1, Ordering::Relaxed));
        let game = paths::instance_game_dir(&id).unwrap();
        paths::ensure_dir(&game.join("mods")).unwrap();
        paths::ensure_dir(&game.join("saves").join("Welt")).unwrap();
        paths::ensure_dir(&game.join("logs")).unwrap();
        std::fs::write(game.join("options.txt"), b"renderDistance:3").unwrap();
        std::fs::write(game.join("mods").join("a.jar"), b"mod").unwrap();
        std::fs::write(game.join("saves").join("Welt").join("level.dat"), vec![0u8; 4096]).unwrap();
        std::fs::write(game.join("logs").join("latest.log"), vec![0u8; 8192]).unwrap();
        id
    }

    fn aufraeumen(id: &str) {
        let _ = std::fs::remove_dir_all(paths::instance_dir(id).unwrap());
        let _ = std::fs::remove_dir_all(paths::snapshots().unwrap().join(id));
    }

    #[test]
    fn sicherung_ohne_welten_ist_kleiner() {
        let id = temp("groesse");
        let mit = create(&id, "mit", true, false).unwrap();
        let ohne = create(&id, "ohne", false, false).unwrap();
        assert!(ohne.size_bytes < mit.size_bytes,
                "ohne Welten muss kleiner sein: {} vs {}", ohne.size_bytes, mit.size_bytes);
        aufraeumen(&id);
    }

    #[test]
    fn sicherungen_werden_gefuehrt_und_geloescht() {
        let id = temp("liste");
        let s = create(&id, "Test", false, false).unwrap();
        assert_eq!(list(&id).len(), 1);
        assert_eq!(list(&id)[0].label, "Test");

        delete(&id, &s.id).unwrap();
        assert!(list(&id).is_empty());
        aufraeumen(&id);
    }

    #[test]
    fn ohne_namen_bekommt_einen_platzhalter() {
        let id = temp("namenlos");
        let s = create(&id, "   ", false, false).unwrap();
        assert_eq!(s.label, "Ohne Namen");
        aufraeumen(&id);
    }

    #[test]
    fn aufraeumen_verschont_haendische_sicherungen() {
        let id = temp("prune");
        create(&id, "auto 1", false, true).unwrap();
        create(&id, "von Hand", false, false).unwrap();
        create(&id, "auto 2", false, true).unwrap();
        create(&id, "auto 3", false, true).unwrap();

        let entfernt = prune(&id, 1).unwrap();
        assert_eq!(entfernt, 2, "zwei der drei automatischen müssen weg");

        let rest = list(&id);
        assert!(rest.iter().any(|s| s.label == "von Hand"),
                "die händische Sicherung muss bleiben");
        assert_eq!(rest.iter().filter(|s| s.automatic).count(), 1);
        aufraeumen(&id);
    }
}
