//! Optionale Unterstuetzung durch ein Sprachmodell.
//!
//! Drei abgegrenzte Aufgaben, jede einzeln abschaltbar:
//!
//! * `log`      -- Absturzursache benennen, wenn die Regeltafel nicht reicht
//! * `search`   -- eine Absicht in Suchanfragen uebersetzen
//! * `conflict` -- erklaeren, warum zwei Mods sich stossen
//!
//! Ohne hinterlegten Schluessel meldet jede Aufgabe `None`. Der Launcher
//! bleibt dann vollstaendig bedienbar -- die Regeltafel, die Stichwortsuche
//! und die rohe Konfliktliste sind die Voreinstellung, nicht der Notnagel.

pub mod capabilities;
pub mod client;
pub mod keys;
pub mod throttle;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::launch::diagnose::Diagnosis;

pub use client::{list_models, ModelInfo, DEFAULT_MODEL};

/// Was die Oberflaeche ueber den Zustand der Anbindung wissen muss.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiStatus {
    pub configured: bool,
    /// Abgeblendeter Schluessel, nie der volle Wert.
    pub key_hint: Option<String>,
    pub model: String,
    /// Kommt der Schluessel aus der Umgebung statt aus der Ablage?
    pub from_env: bool,
    /// Parameter, die dieses Modell abgelehnt hat und die deshalb nicht
    /// mehr gesendet werden.
    pub rejected: Vec<String>,
}

pub fn status(model: &str) -> AiStatus {
    AiStatus {
        configured: keys::is_configured(),
        key_hint: keys::masked(),
        model: model.to_string(),
        from_env: std::env::var(keys::ENV_VAR).is_ok_and(|v| !v.trim().is_empty()),
        rejected: client::capabilities_of(model),
    }
}

/* ---- Absturzanalyse --------------------------------------------------- */

/// Laesst das Modell die Vorpruefung bestaetigen oder widerlegen.
///
/// Scheitert der Aufruf -- kein Schluessel, kein Netz, Ratengrenze
/// ausgereizt -- bleibt es beim Ergebnis der Regeltafel. Ein Fehlschlag
/// hier darf den Nutzer nie ohne Diagnose zuruecklassen.
pub async fn analyze_log(
    model: &str,
    log: &str,
    fallback: Option<&Diagnosis>,
) -> Option<Diagnosis> {
    let key = keys::api_key()?;
    match client::analyze(&key, model, log, fallback).await {
        Ok(d) => Some(d),
        Err(e) => {
            eprintln!("[ai] Absturzanalyse fehlgeschlagen: {e}");
            None
        }
    }
}

/* ---- Suche in natuerlicher Sprache ------------------------------------ */

/// Das Ergebnis der Absichtserkennung.
///
/// Bewusst keine Projektnamen vom Modell: es liefert Suchanfragen und
/// Merkmale, gesucht wird weiterhin bei Modrinth und CurseForge. So
/// koennen keine Mods erfunden werden, die es nicht gibt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchIntent {
    /// Suchanfragen, in absteigender Wichtigkeit.
    pub queries: Vec<String>,
    /// Erkannter Inhaltstyp, falls die Formulierung ihn hergibt.
    pub project_type: Option<String>,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    /// Was der Nutzer laut Modell erreichen will -- wird angezeigt,
    /// damit eine Fehldeutung sofort auffaellt.
    pub reading: String,
}

const SEARCH_PROMPT: &str = "\
Du übersetzt den Wunsch eines Minecraft-Spielers in Suchanfragen für die \
Mod-Portale Modrinth und CurseForge.

Antworte ausschließlich mit einem JSON-Objekt, ohne Text davor oder danach. \
Felder:
  queries       2 bis 5 kurze englische Suchbegriffe, wichtigste zuerst.
                Englisch, weil die Portale englisch indexieren.
  project_type  \"mod\", \"modpack\", \"resourcepack\", \"shader\" oder \"datapack\", sonst null.
  game_version  Minecraft-Version, falls genannt, sonst null.
  loader        \"fabric\", \"forge\", \"neoforge\", \"quilt\", falls genannt, sonst null.
  reading       Ein deutscher Satz: wie du den Wunsch verstanden hast.

Erfinde keine Mod-Namen. Gib Suchbegriffe an, keine Projekte.";

pub async fn interpret_search(model: &str, wish: &str) -> Result<Option<SearchIntent>> {
    let Some(key) = keys::api_key() else {
        return Ok(None);
    };
    let raw = client::ask_json(&key, model, SEARCH_PROMPT, wish).await?;
    Ok(Some(serde_json::from_str(&raw)?))
}

/* ---- Konflikte erklaeren ---------------------------------------------- */

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictExplanation {
    /// Worum es geht, in einem Satz.
    pub summary: String,
    /// Warum die beiden sich stossen.
    pub detail: String,
    /// Handlungsvorschlaege, sinnvollste zuerst.
    pub options: Vec<String>,
}

const CONFLICT_PROMPT: &str = "\
Du erklärst einem Minecraft-Spieler, warum eine Mod-Installation stockt.

Antworte ausschließlich mit einem JSON-Objekt, ohne Text davor oder danach. \
Felder:
  summary  Ein deutscher Satz: worum es geht.
  detail   Zwei bis vier deutsche Sätze: warum das Problem auftritt.
  options  2 bis 4 konkrete deutsche Handlungsvorschläge, bester zuerst.

Bleib bei dem, was in den Angaben steht. Erfinde keine Versionsnummern \
und keine Mod-Namen.";

pub async fn explain_conflict(model: &str, facts: &str) -> Result<Option<ConflictExplanation>> {
    let Some(key) = keys::api_key() else {
        return Ok(None);
    };
    let raw = client::ask_json(&key, model, CONFLICT_PROMPT, facts).await?;
    Ok(Some(serde_json::from_str(&raw)?))
}
