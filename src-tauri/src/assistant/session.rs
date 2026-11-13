//! Der Ablauf als Zustand.
//!
//! Der Assistent führt durch mehrere Schritte, und dazwischen kann viel
//! passieren: das Ratenlimit greift, das Netz fällt aus, der Launcher wird
//! geschlossen. Deshalb liegt der Stand auf der Platte und lässt sich
//! fortsetzen -- eine halb beantwortete Sitzung zu verlieren wäre
//! ärgerlich, gerade weil die Fragen Nachdenken kosten.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::questions::{self, Question};
use super::search::Candidate;
use crate::error::{Error, Result};
use crate::paths;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// Der Nutzer beschreibt, was er vorhat.
    Describe,
    /// Fragen beantworten.
    Answer,
    /// Vorschläge auswählen.
    Choose,
    /// Plan liegt vor, wartet auf Bestätigung.
    Review,
    /// Umgesetzt.
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    /// Die freie Beschreibung. Mehrzeilig -- sie ist die Grundlage, an der
    /// das Modell den Katalog zuschneidet, und bleibt ohne Schlüssel als
    /// Notiz an der Instanz.
    #[serde(default)]
    pub goal: String,
    pub step: Step,
    #[serde(default)]
    pub questions: Vec<Question>,
    /// Frage-Kennung -> gewählte Antworten.
    #[serde(default)]
    pub answers: BTreeMap<String, Vec<String>>,
    #[serde(default)]
    pub candidates: Vec<Candidate>,
    /// Ob die Suche gelaufen ist. Ohne das laesst sich eine Suche, die
    /// nichts gefunden hat, nicht von einer unterscheiden, die noch
    /// laeuft -- die Oberflaeche drehte dann ewig weiter.
    #[serde(default)]
    pub searched: bool,
    /// Projektkennungen, die installiert werden sollen.
    #[serde(default)]
    pub selected: Vec<String>,
    #[serde(default)]
    pub plan: Option<Plan>,
    /// Womit der Katalog erzeugt wurde: `modell` oder `katalog`.
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub created_at: String,
    /// Hinweise aus der Auswertung.
    #[serde(default)]
    pub notes: Vec<String>,
}

/// Was am Ende angelegt wird.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Plan {
    pub name: String,
    pub mc_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub min_ram: u32,
    pub max_ram: u32,
    pub optimization: Option<String>,
    /// Gedeckelt und synchronisiert, oder ungebremst. Kommt aus den
    /// Antworten und darf beim Umsetzen nicht verlorengehen.
    #[serde(default)]
    pub frame_cap: Option<u32>,
    pub jvm_args: Vec<String>,
    pub mods: Vec<PlannedMod>,
    /// Begründung in eigenen Worten, sofern ein Modell beteiligt war.
    pub rationale: Option<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlannedMod {
    pub project_id: String,
    pub title: String,
    pub icon_url: Option<String>,
    /// Warum diese Mod im Plan steht.
    pub reason: String,
    /// Von einer Rolle der Optimierungsstufe beigesteuert, nicht gewählt.
    #[serde(default)]
    pub from_optimization: bool,
}

impl Session {
    pub fn new(goal: &str) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            goal: goal.trim().to_string(),
            step: if goal.trim().is_empty() { Step::Describe } else { Step::Answer },
            questions: Vec::new(),
            answers: BTreeMap::new(),
            candidates: Vec::new(),
            searched: false,
            selected: Vec::new(),
            plan: None,
            source: String::new(),
            created_at: crate::instance::now(),
            notes: Vec::new(),
        }
    }

    /// Sind alle Fragen beantwortet?
    ///
    /// Fragen, deren Antwort durch eine frühere hinfällig wurde, zählen
    /// nicht -- wer keinen Loader nimmt, muss nichts zu Mods sagen.
    pub fn complete(&self) -> bool {
        self.questions
            .iter()
            .filter(|q| self.applies(&q.id))
            .all(|q| self.answers.get(&q.id).is_some_and(|a| !a.is_empty()))
    }

    /// Gilt diese Frage angesichts der bisherigen Antworten?
    pub fn applies(&self, question_id: &str) -> bool {
        let vanilla = self
            .answers
            .get("loader")
            .is_some_and(|a| a.iter().any(|v| v == "vanilla"));

        // Ohne Loader gibt es keine Mods -- die Modfragen entfallen.
        if vanilla {
            return !matches!(question_id, "ziel" | "komfort");
        }
        true
    }

    /// Die nächste unbeantwortete Frage.
    pub fn next_question(&self) -> Option<&Question> {
        self.questions
            .iter()
            .filter(|q| self.applies(&q.id))
            .find(|q| !self.answers.contains_key(&q.id))
    }

    pub fn answer(&mut self, question_id: &str, choices: Vec<String>) -> Result<()> {
        if !self.questions.iter().any(|q| q.id == question_id) {
            return Err(Error::not_found(format!("Frage {question_id} gibt es nicht")));
        }
        if choices.is_empty() {
            return Err(Error::other("Bitte wähle mindestens eine Antwort"));
        }
        self.answers.insert(question_id.to_string(), choices);
        Ok(())
    }

    /// Antworten in der Reihenfolge des Katalogs.
    pub fn answer_pairs(&self) -> Vec<(String, Vec<String>)> {
        self.questions
            .iter()
            .filter_map(|q| self.answers.get(&q.id).map(|a| (q.id.clone(), a.clone())))
            .collect()
    }

    pub fn outcome(&self) -> questions::Outcome {
        questions::evaluate_in(&self.questions, &self.answer_pairs())
    }
}

/* ---- Ablage ------------------------------------------------------------- */

fn dir() -> Result<std::path::PathBuf> {
    let d = paths::root()?.join("assistant");
    paths::ensure_dir(&d)?;
    Ok(d)
}

fn file(id: &str) -> Result<std::path::PathBuf> {
    // Die Kennung kommt von uns, aber geprueft wird trotzdem.
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(Error::other("Ungültige Sitzungskennung"));
    }
    Ok(dir()?.join(format!("{id}.json")))
}

pub fn save(session: &Session) -> Result<()> {
    crate::store::write(&file(&session.id)?, session)
}

pub fn load(id: &str) -> Result<Session> {
    crate::store::read(&file(id)?)
        .map_err(|_| Error::not_found("Diese Sitzung gibt es nicht mehr"))
}

pub fn delete(id: &str) -> Result<()> {
    let p = file(id)?;
    if p.exists() {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

/// Unfertige Sitzungen, neueste zuerst.
pub fn list_open() -> Vec<Session> {
    let Ok(d) = dir() else { return Vec::new() };
    let mut out: Vec<Session> = std::fs::read_dir(d)
        .map(|it| {
            it.filter_map(|e| e.ok())
                .filter_map(|e| crate::store::read::<Session>(&e.path()).ok())
                .filter(|s| s.step != Step::Done)
                .collect()
        })
        .unwrap_or_default();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assistant::questions::Context;

    fn mit_fragen() -> Session {
        let mut s = Session::new("Ich will ein Technik-Paket.");
        s.questions = questions::catalogue(&Context::default());
        s
    }

    #[test]
    fn beschreibung_ueberspringt_den_ersten_schritt() {
        assert_eq!(Session::new("etwas").step, Step::Answer);
        assert_eq!(Session::new("   ").step, Step::Describe);
    }

    #[test]
    fn unbeantwortete_frage_wird_gefunden() {
        let mut s = mit_fragen();
        assert_eq!(s.next_question().unwrap().id, "version");
        s.answer("version", vec!["1.21.4".into()]).unwrap();
        assert_eq!(s.next_question().unwrap().id, "loader");
    }

    #[test]
    fn ohne_loader_entfallen_die_modfragen() {
        let mut s = mit_fragen();
        s.answer("version", vec!["1.21.4".into()]).unwrap();
        s.answer("loader", vec!["vanilla".into()]).unwrap();

        assert!(!s.applies("ziel"), "ohne Loader gibt es keine Mods");
        assert!(!s.applies("komfort"));
        // Gerät und Arbeitsspeicher bleiben trotzdem gefragt.
        assert!(s.applies("geraet"));
        assert!(s.applies("ram"));

        let offen: Vec<&str> = std::iter::from_fn(|| s.next_question().map(|q| q.id.as_str()))
            .take(1).collect();
        assert!(!offen.contains(&"ziel"));
    }

    #[test]
    fn leere_antwort_wird_abgewiesen() {
        let mut s = mit_fragen();
        assert!(s.answer("version", vec![]).is_err());
    }

    #[test]
    fn unbekannte_frage_wird_abgewiesen() {
        let mut s = mit_fragen();
        assert!(s.answer("gibt_es_nicht", vec!["x".into()]).is_err());
    }

    #[test]
    fn vollstaendig_erst_wenn_alles_geltende_beantwortet_ist() {
        let mut s = mit_fragen();
        assert!(!s.complete());
        for q in s.questions.clone() {
            s.answer(&q.id, vec![q.options[0].id.clone()]).unwrap();
        }
        assert!(s.complete());
    }

    #[test]
    fn kennung_mit_pfad_wird_abgewiesen() {
        assert!(file("../../etc/passwd").is_err());
        assert!(file("a/b").is_err());
    }
}
