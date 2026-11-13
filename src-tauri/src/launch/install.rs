//! Eine Version vollstaendig bereitstellen.
//!
//! Reihenfolge ist wichtig: erst die Versions-JSON (sie sagt, was gebraucht
//! wird), dann parallel Client-Jar, Bibliotheken und Assets, zuletzt die
//! nativen Bibliotheken auspacken. Alles bereits Vorhandene wird
//! uebersprungen, ohne dass eine Anfrage entsteht.

use std::path::PathBuf;
use std::sync::Arc;

use crate::download::{self, plan, Item, Sink};
use crate::error::{Error, Result};
use crate::manifest::{self, rules::RuleContext, VersionInfo};
use crate::paths;

pub struct Installed {
    pub info: VersionInfo,
    /// Pfade der Jar-Dateien im Klassenpfad, in dieser Reihenfolge.
    pub classpath: Vec<PathBuf>,
    /// Verzeichnis mit den entpackten nativen Bibliotheken.
    pub natives_dir: PathBuf,
}

/// Stellt sicher, dass die Version startbereit ist.
pub async fn ensure(
    version_id: &str,
    concurrency: usize,
    verify: bool,
    sink: Arc<dyn Sink>,
) -> Result<Installed> {
    let info = manifest::resolve(version_id).await?;
    let ctx = RuleContext::current();

    let libs = plan::libraries(&info, &ctx)?;

    // Client-Jar und Bibliotheken in einem Durchgang -- sie liegen auf
    // demselben Server, getrennte Durchgaenge braeuchten nur mehr Zeit.
    let mut items: Vec<Item> = libs.iter().map(|(i, _)| i.clone()).collect();
    if let Some(jar) = plan::client_jar(&info)? {
        items.push(jar);
    }

    download::fetch_all(
        items, concurrency, verify,
        "Version wird geladen", "version", sink.clone(),
    ).await?;

    let (assets, _index) = plan::assets(&info).await?;
    download::fetch_all(
        assets, concurrency, verify,
        "Spielressourcen werden geladen", "assets", sink,
    ).await?;

    // Natives je Version auspacken: zwei Versionen koennen
    // unterschiedliche LWJGL-Fassungen brauchen.
    let natives_dir = paths::natives()?.join(&info.id);
    let native_archives: Vec<PathBuf> = libs
        .iter()
        .filter(|(_, is_native)| *is_native)
        .map(|(i, _)| i.target.clone())
        .collect();
    // Entpacken (Zip-Dekompression + Dateischreiben, oft ein paar Dutzend
    // Archive) ist blockierend -- direkt in dieser async fn haette es
    // denselben Tokio-Arbeitsthread belegen und, bei genug Mods/Natives,
    // die Tauri-Oberflaeche einfrieren lassen koennen, wie schon einmal
    // bei der Sicherheitspruefung.
    let natives_dir_fuer_blocking = natives_dir.clone();
    let archive_liste = native_archives.clone();
    tokio::task::spawn_blocking(move || plan::extract_natives(&archive_liste, &natives_dir_fuer_blocking))
        .await
        .map_err(|e| Error::other(format!("Entpacken der nativen Bibliotheken abgebrochen: {e}")))??;

    // Klassenpfad: nur die gewoehnlichen Bibliotheken, keine Natives.
    let mut classpath: Vec<PathBuf> = libs
        .iter()
        .filter(|(_, is_native)| !*is_native)
        .map(|(i, _)| i.target.clone())
        .collect();
    classpath.push(paths::version_jar(&info.id)?);

    Ok(Installed { info, classpath, natives_dir })
}
