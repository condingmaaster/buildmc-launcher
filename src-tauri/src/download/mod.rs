//! Nebenlaeufiges Herunterladen mit Pruefsummenkontrolle.
//!
//! Eine Minecraft-Installation besteht aus einigen tausend sehr kleinen
//! Dateien (allein die Assets). Entscheidend ist deshalb nicht die
//! Bandbreite, sondern wie viele Anfragen gleichzeitig offen sind und dass
//! bereits vorhandene Dateien gar nicht erst angefasst werden.

pub mod plan;

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use futures::stream::{FuturesUnordered, StreamExt};
use serde::Serialize;
use sha1::{Digest, Sha1};
use tokio::io::AsyncWriteExt;

use crate::error::{Error, Result};
use crate::{http, paths};

/// Eine geplante Datei.
#[derive(Debug, Clone)]
pub struct Item {
    pub url: String,
    pub target: PathBuf,
    /// Erwartete SHA1-Summe, sofern das Manifest eine nennt.
    pub sha1: Option<String>,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub id: String,
    pub label: String,
    pub detail: String,
    pub current: u64,
    pub total: u64,
    pub bytes_per_second: u64,
    pub done: bool,
    pub error: Option<String>,
}

/// Empfaengt Fortschritt. In der Anwendung ein Tauri-Ereignis, im Test
/// eine einfache Ablage -- deshalb als Merkmal und nicht fest verdrahtet.
pub trait Sink: Send + Sync + 'static {
    fn emit(&self, p: &Progress);
}

/// Verwirft jede Meldung. Fuer Aufrufe ohne Oberflaeche.
pub struct Silent;
impl Sink for Silent {
    fn emit(&self, _: &Progress) {}
}

/// Laedt alle Eintraege. Bereits vorhandene und korrekte Dateien werden
/// uebersprungen, ohne dass eine Anfrage entsteht.
pub async fn fetch_all(
    items: Vec<Item>,
    concurrency: usize,
    verify: bool,
    label: &str,
    id: &str,
    sink: Arc<dyn Sink>,
) -> Result<()> {
    let pending: Vec<Item> = items
        .into_iter()
        .filter(|i| !is_satisfied(i, verify))
        .collect();

    let total = pending.len() as u64;
    if total == 0 {
        sink.emit(&Progress {
            id: id.into(),
            label: label.into(),
            detail: "bereits vollständig".into(),
            current: 0,
            total: 0,
            bytes_per_second: 0,
            done: true,
            error: None,
        });
        return Ok(());
    }

    let done = Arc::new(AtomicU64::new(0));
    let bytes = Arc::new(AtomicU64::new(0));
    let started = std::time::Instant::now();

    let mut queue = pending.into_iter();
    let mut running = FuturesUnordered::new();

    // Erst das Fenster fuellen, dann fuer jede fertige Aufgabe eine neue
    // nachschieben -- so sind nie mehr als `concurrency` Anfragen offen.
    for _ in 0..concurrency.max(1) {
        match queue.next() {
            Some(item) => running.push(spawn_one(item, verify, done.clone(), bytes.clone())),
            None => break,
        }
    }

    let mut last_emit = std::time::Instant::now();

    while let Some(result) = running.next().await {
        let name = result?;

        if let Some(item) = queue.next() {
            running.push(spawn_one(item, verify, done.clone(), bytes.clone()));
        }

        let current = done.load(Ordering::Relaxed);
        let elapsed = started.elapsed().as_secs_f64().max(0.001);

        // Hoechstens 20 Meldungen je Sekunde: mehr sieht niemand, und die
        // Oberflaeche soll nicht bei jedem Asset neu zeichnen.
        if last_emit.elapsed().as_millis() >= 50 || current == total {
            last_emit = std::time::Instant::now();
            sink.emit(&Progress {
                id: id.into(),
                label: label.into(),
                detail: name,
                current,
                total,
                bytes_per_second: (bytes.load(Ordering::Relaxed) as f64 / elapsed) as u64,
                done: current == total,
                error: None,
            });
        }
    }

    Ok(())
}

fn spawn_one(
    item: Item,
    verify: bool,
    done: Arc<AtomicU64>,
    bytes: Arc<AtomicU64>,
) -> impl std::future::Future<Output = Result<String>> {
    async move {
        let name = item
            .target
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();

        let written = fetch_one(&item, verify).await?;
        done.fetch_add(1, Ordering::Relaxed);
        bytes.fetch_add(written, Ordering::Relaxed);
        Ok(name)
    }
}

/// Wie oft ein einzelner Download versucht wird, bevor der Fehler den
/// ganzen Stapel abbricht. Bei tausenden kleinen Asset-Dateien reicht sonst
/// ein einziger kurzer Netzwerkaussetzer, um eine sonst fast vollstaendige
/// Installation komplett scheitern zu lassen.
const MAX_VERSUCHE: u32 = 3;

/// Laedt eine Datei, mit Wiederholung bei einem Fehlschlag.
async fn fetch_one(item: &Item, verify: bool) -> Result<u64> {
    let mut letzter_fehler = None;
    for versuch in 0..MAX_VERSUCHE {
        if versuch > 0 {
            // Kurz warten statt sofort erneut zu versuchen -- ein
            // Netzwerkaussetzer braucht oft einen Moment, um sich zu legen.
            let wartezeit = 300u64 * 2u64.pow(versuch - 1);
            tokio::time::sleep(std::time::Duration::from_millis(wartezeit)).await;
        }
        match fetch_one_versuch(item, verify).await {
            Ok(n) => return Ok(n),
            Err(e) => letzter_fehler = Some(e),
        }
    }
    Err(letzter_fehler.expect("mindestens ein Versuch lief"))
}

/// Genau ein Versuch. Geschrieben wird erst in eine Nebendatei und dann
/// umbenannt -- ein Abbruch hinterlaesst so keine halbe Datei, die beim
/// naechsten Start als vollstaendig gelten wuerde.
async fn fetch_one_versuch(item: &Item, verify: bool) -> Result<u64> {
    paths::ensure_parent(&item.target)?;

    let tmp = item.target.with_extension("part");
    let mut response = http::client()
        .get(&item.url)
        .send()
        .await?
        .error_for_status()?;

    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha1::new();
    let mut written = 0u64;

    while let Some(chunk) = response.chunk().await? {
        if verify {
            hasher.update(&chunk);
        }
        file.write_all(&chunk).await?;
        written += chunk.len() as u64;
    }
    file.flush().await?;
    drop(file);

    if verify {
        if let Some(expected) = &item.sha1 {
            let got = hex::encode(hasher.finalize());
            if &got != expected {
                let _ = tokio::fs::remove_file(&tmp).await;
                return Err(Error::Checksum {
                    datei: item.target.display().to_string(),
                    erwartet: expected.clone(),
                    erhalten: got,
                });
            }
        }
    }

    tokio::fs::rename(&tmp, &item.target).await?;
    Ok(written)
}

/// Liegt die Datei bereits korrekt vor?
///
/// Ohne Pruefsummenkontrolle entscheidet die Groesse; das Manifest nennt
/// sie fuer nahezu jede Datei und es kostet keinen Lesevorgang.
fn is_satisfied(item: &Item, verify: bool) -> bool {
    let Ok(meta) = std::fs::metadata(&item.target) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    if item.size > 0 && meta.len() != item.size {
        return false;
    }
    if !verify {
        return true;
    }
    match &item.sha1 {
        None => true,
        Some(expected) => file_sha1(&item.target).map(|got| &got == expected).unwrap_or(false),
    }
}

pub fn file_sha1(path: &std::path::Path) -> Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha1::new();
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vorhandene_datei_mit_falscher_groesse_gilt_als_fehlend() {
        let dir = std::env::temp_dir().join("buildmc-test-dl");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.bin");
        std::fs::write(&f, b"hallo").unwrap();

        let item = Item { url: String::new(), target: f.clone(), sha1: None, size: 99 };
        assert!(!is_satisfied(&item, false));

        let item = Item { size: 5, ..item };
        assert!(is_satisfied(&item, false));

        std::fs::remove_file(&f).ok();
    }

    #[test]
    fn sha1_wird_geprueft() {
        let dir = std::env::temp_dir().join("buildmc-test-dl");
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("b.bin");
        std::fs::write(&f, b"abc").unwrap();

        // SHA1("abc")
        let want = "a9993e364706816aba3e25717850c26c9cd0d89d";
        assert_eq!(file_sha1(&f).unwrap(), want);

        let item = Item {
            url: String::new(),
            target: f.clone(),
            sha1: Some(want.into()),
            size: 3,
        };
        assert!(is_satisfied(&item, true));

        let bad = Item { sha1: Some("0".repeat(40)), ..item };
        assert!(!is_satisfied(&bad, true));

        std::fs::remove_file(&f).ok();
    }
}
