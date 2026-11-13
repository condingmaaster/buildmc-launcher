//! Anbindung an VirusTotal -- Zweitmeinung per Hash-Abfrage.
//!
//! Bewusst nur eine **Abfrage**, kein aktiver Wächter: VirusTotal ist ein
//! passiver Dienst, der einen bereits bekannten Hash gegen ~70 Antiviren-
//! Engines abgleicht. Er ersetzt keinen Echtzeitschutz -- er beantwortet
//! nur die Frage "wurde diese genaue Datei schon einmal irgendwo als
//! bösartig gemeldet?".
//!
//! Geprüft, nicht angenommen: `GET /api/v3/files/{hash}`, Kopf `x-apikey`,
//! Antwort unter `data.attributes.last_analysis_stats`.

use std::collections::VecDeque;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde::Deserialize;
use tokio::sync::Mutex;

use crate::error::Result;
use crate::http;

const BASE: &str = "https://www.virustotal.com/api/v3";
const HEADER: &str = "x-apikey";

/// Kostenlose Stufe: 4 Anfragen je Minute. Fest, nicht aus Antwortkoepfen
/// gelernt -- anders als bei Mistral ist dieser Wert dokumentiert und
/// aendert sich nicht je Anfrage.
const FENSTER: Duration = Duration::from_secs(60);
const ANFRAGEN_JE_FENSTER: usize = 4;

fn zeitstempel() -> &'static Mutex<VecDeque<Instant>> {
    static SPEICHER: OnceLock<Mutex<VecDeque<Instant>>> = OnceLock::new();
    SPEICHER.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// Wartet, bis eine Anfrage ins Zeitfenster passt -- entscheidend, sobald
/// mehrere Dateien gleichzeitig geprueft werden (siehe `security::mod`):
/// ohne das wuerde eine Instanz mit mehr als vier Mods sofort ueber die
/// Grenze laufen, weil alle Abfragen im selben Augenblick losgehen.
async fn warte_auf_slot() {
    loop {
        let warten = {
            let mut zeiten = zeitstempel().lock().await;
            let jetzt = Instant::now();
            while zeiten.front().is_some_and(|t| jetzt.duration_since(*t) >= FENSTER) {
                zeiten.pop_front();
            }
            if zeiten.len() < ANFRAGEN_JE_FENSTER {
                zeiten.push_back(jetzt);
                None
            } else {
                Some(FENSTER - jetzt.duration_since(*zeiten.front().unwrap()))
            }
        };
        match warten {
            None => return,
            Some(dauer) => tokio::time::sleep(dauer).await,
        }
    }
}

/// Ab wie vielen Engines ein Fund als echtes Signal statt als Rauschen
/// gilt. Bei rund 70 Scannern ist ein einzelner Treffer fast immer ein
/// Fehlalarm einer einzelnen heuristischen Engine -- zwei unabhängige
/// Engines, die unabhängig voneinander dasselbe melden, sind es nicht
/// mehr. Dieselbe Denkweise wie die 40%-Schwelle in `launch/diagnose.rs`.
const AUFFAELLIG_AB: u32 = 2;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    /// Kein einziger Treffer -- oder zu wenige, um als Signal zu zaehlen.
    Clean { engines: u32 },
    /// Genug unabhaengige Treffer fuer ein echtes Signal.
    Flagged { engines: u32, total: u32 },
    /// VirusTotal kennt diesen Hash nicht -- weder gut noch schlecht.
    Unknown,
}

#[derive(Debug, Deserialize)]
struct FileResponse {
    data: FileData,
}

#[derive(Debug, Deserialize)]
struct FileData {
    attributes: Attributes,
}

#[derive(Debug, Deserialize)]
struct Attributes {
    last_analysis_stats: Stats,
}

#[derive(Debug, Deserialize)]
struct Stats {
    malicious: u32,
    suspicious: u32,
    #[serde(default)]
    harmless: u32,
    #[serde(default)]
    undetected: u32,
}

fn urteil(stats: &Stats) -> Verdict {
    // "suspicious" zaehlt mit -- eine Engine, die nur "verdaechtig" statt
    // "boesartig" meldet, ist trotzdem ein Datenpunkt, kein Rauschen.
    let engines = stats.malicious + stats.suspicious;
    let total = engines + stats.harmless + stats.undetected;
    if engines >= AUFFAELLIG_AB {
        Verdict::Flagged { engines, total }
    } else {
        Verdict::Clean { engines }
    }
}

/// Fragt einen Hash ab. Ohne Schluessel `Verdict::Unknown` -- die Pruefung
/// ist freiwillig, kein Fehler ohne Schluessel.
pub async fn lookup(sha1: &str) -> Result<Verdict> {
    let Some(key) = super::keys::api_key() else {
        return Ok(Verdict::Unknown);
    };
    warte_auf_slot().await;

    let url = format!("{BASE}/files/{sha1}");
    let response = http::client().get(&url).header(HEADER, &key).send().await?;

    if response.status() == reqwest::StatusCode::NOT_FOUND {
        // VirusTotal kennt die Datei schlicht nicht -- kein Fehler.
        return Ok(Verdict::Unknown);
    }
    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(crate::Error::other("Der VirusTotal-Schlüssel wurde abgelehnt"));
    }
    if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(crate::Error::other(
            "VirusTotal-Ratengrenze erreicht (4 Anfragen/Minute in der kostenlosen Stufe)",
        ));
    }

    let parsed: FileResponse = response.error_for_status()?.json().await?;
    Ok(urteil(&parsed.data.attributes.last_analysis_stats))
}

/// Laedt eine bislang unbekannte Datei zur Analyse hoch -- nur auf
/// ausdrueckliche Bitte des Nutzers je Datei, nie automatisch: der
/// Dateiinhalt wird danach bei VirusTotal oeffentlich einsehbar.
pub async fn upload(path: &std::path::Path) -> Result<()> {
    let key = super::keys::api_key()
        .ok_or_else(|| crate::Error::not_found("Kein VirusTotal-Schlüssel hinterlegt"))?;
    let bytes = tokio::fs::read(path).await?;
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("datei").to_string();
    let part = reqwest::multipart::Part::bytes(bytes).file_name(filename);
    let form = reqwest::multipart::Form::new().part("file", part);

    http::client()
        .post(format!("{BASE}/files"))
        .header(HEADER, &key)
        .multipart(form)
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(malicious: u32, suspicious: u32) -> Stats {
        Stats { malicious, suspicious, harmless: 60, undetected: 5 }
    }

    #[test]
    fn kein_treffer_gilt_als_unbedenklich() {
        assert_eq!(urteil(&stats(0, 0)), Verdict::Clean { engines: 0 });
    }

    #[test]
    fn ein_einzelner_treffer_ist_noch_kein_signal() {
        // Bei ~70 Engines ist ein einzelner Fehlalarm haeufig.
        assert_eq!(urteil(&stats(1, 0)), Verdict::Clean { engines: 1 });
    }

    #[test]
    fn zwei_unabhaengige_treffer_gelten_als_signal() {
        assert_eq!(urteil(&stats(2, 0)), Verdict::Flagged { engines: 2, total: 67 });
    }

    #[test]
    fn verdaechtig_zaehlt_zusammen_mit_boesartig() {
        assert_eq!(urteil(&stats(1, 1)), Verdict::Flagged { engines: 2, total: 67 });
    }

    #[test]
    fn nur_verdaechtige_treffer_zaehlen_ebenso() {
        assert_eq!(urteil(&stats(0, 2)), Verdict::Flagged { engines: 2, total: 67 });
    }
}
