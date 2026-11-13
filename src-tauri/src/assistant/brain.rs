//! Die beiden Modellanfragen.
//!
//! Mehr sind es bewusst nicht: der Katalog wird einmal zugeschnitten, der
//! Plan einmal abgewogen. Die Suche dazwischen läuft ohne Modell, weil sie
//! eine Zuordnung ist und keine Deutung -- und jede Anfrage dort später am
//! Ratenlimit fehlen würde.
//!
//! Beide Aufrufe dürfen ausfallen. Dann greift der Weg ohne Modell, und
//! der Nutzer erfährt es, statt eine schlechtere Antwort untergeschoben
//! zu bekommen.

use serde::Deserialize;

use super::questions::{self, Context, Question};
use super::search::Candidate;
use super::session::{Plan, Session};
use crate::ai;
use crate::error::Result;

const CATALOGUE_BASE: &str = "\
Du bereitest einen Fragebogen vor, mit dem ein Minecraft-Launcher eine neue \
Instanz einrichtet.

Du bekommst die Beschreibung des Nutzers und einen Standardkatalog. Schneide \
den Katalog darauf zu: streiche, was die Beschreibung schon beantwortet, \
ergänze höchstens zwei Fragen, die hier wirklich nötig sind, und passe die \
Formulierungen an.";

/// Nur mit aktivierter Recherche: der Zuschnitt darf zum eigenen Entwurf
/// werden, nicht nur zur gekuerzten Kopie. Die harte Grenze -- keine
/// erfundenen Mod-Namen -- gilt unveraendert weiter, sie steht in
/// CATALOGUE_BASE_ENDE unten.
const CATALOGUE_RESEARCH: &str = "\
Du darfst dabei weiter gehen als sonst: uebernimm, verwirf, formuliere um \
oder erfinde neue Fragen, wie es der Beschreibung am besten dient -- \
gestalte den Fragebogen, nicht nur den Zuschnitt. Nutze die Websuche, um \
aktuelle, verbreitete Mod-Kategorien und Empfehlungen fuer die genannte \
Minecraft-Version und den Lader einzuordnen, bevor du die Fragen formulierst.";

const CATALOGUE_ENDE: &str = "\n\nAntworte ausschließlich mit einem JSON-Objekt, ohne Text davor oder danach:
  questions  Liste der Fragen. Jede: id, prompt, why, multi (true/false),
             options (Liste mit id, label, detail, queries).
             `queries` sind kurze englische Suchbegriffe für Mod-Portale.
  reading    Ein deutscher Satz: wie du die Beschreibung verstanden hast.

Behalte die Fragen mit den ids version, loader, geraet und ram unverändert bei \
— sie sind technisch nötig. Erfinde keine Mod-Namen; `queries` sind Suchbegriffe, \
keine Projekte -- das gilt unveraendert, auch mit Websuche: die eigentliche \
Mod-Auswahl kommt immer aus einer echten, separaten Modrinth-Suche, nie aus \
Rechercheergebnissen.

Mindestens eine nicht-technische Frage muss so gestaltet sein, dass jede ihrer \
Optionen — außer einer ausdrücklichen \"nichts davon\"-Option — echte \
Suchbegriffe oder Kategorien trägt. Entfernst du eine Frage, die im \
Standardkatalog Suchbegriffe trug (etwa \"ziel\"), muss eine andere Frage \
diese Rolle übernehmen. Eine Antwort ohne jeden Suchansatz darf am Ende nicht \
herauskommen.";

fn catalogue_prompt(research: bool) -> String {
    if research {
        format!("{CATALOGUE_BASE}\n\n{CATALOGUE_RESEARCH}{CATALOGUE_ENDE}")
    } else {
        format!("{CATALOGUE_BASE}{CATALOGUE_ENDE}")
    }
}

#[derive(Debug, Deserialize)]
struct CatalogueReply {
    #[serde(default)]
    questions: Vec<Question>,
    #[serde(default)]
    reading: String,
}

/// Die vier Fragen, ohne die der Ablauf nicht funktioniert.
const REQUIRED: [&str; 4] = ["version", "loader", "geraet", "ram"];

/// Schneidet den Katalog auf die Beschreibung zu.
///
/// Gibt `None` zurück, wenn kein Schlüssel hinterlegt ist. Fällt der Aufruf
/// aus, gibt er den Fehler weiter -- die Oberfläche fragt dann, ob mit dem
/// Ersatzmodell weitergemacht werden soll.
pub async fn tailor_catalogue(
    model: &str,
    goal: &str,
    ctx: &Context,
    research: bool,
    events: &dyn ai::client::AiEvents,
) -> Result<Option<(Vec<Question>, String)>> {
    let Some(key) = ai::keys::api_key() else {
        return Ok(None);
    };

    let standard = questions::catalogue(ctx);
    let user = format!(
        "Beschreibung des Nutzers:\n\n{goal}\n\nStandardkatalog:\n{}",
        serde_json::to_string_pretty(&standard)?
    );

    let prompt = catalogue_prompt(research);
    // Nur Websuche -- Codeausfuehrung gibt es hier nichts zu berechnen.
    let tools: &[ai::client::Tool] =
        if research { &[ai::client::Tool::WebSearch] } else { &[] };
    let raw =
        ai::client::ask_json_streaming(&key, model, &prompt, &user, tools, "katalog", events)
            .await?;
    let reply: CatalogueReply = serde_json::from_str(&raw)
        .map_err(|e| crate::Error::other(format!("Antwort des Modells unlesbar: {e}")))?;

    let mut out = merge_required(reply.questions, &standard);
    ergaenze_garantierten_suchpfad(&mut out, &standard);
    Ok(Some((out, reply.reading)))
}

/// Traegt keine der nicht-technischen Fragen einen echten Suchbegriff --
/// weil das Modell die einzige Frage mit garantiertem Suchbegriff
/// wegliess oder durch eine rein technische ersetzte --, wird die
/// bewaehrte "ziel"-Frage aus dem Standardkatalog angehaengt. Das ist die
/// zweite von drei Sicherungsschichten gegen eine leere Suche: die erste
/// steht im Prompt (schwach, nur eine Bitte), die dritte in
/// `questions::evaluate_in` (hart, unbedingt). Diese hier ist ein
/// deterministischer Mittelweg ohne weiteren Modellaufruf.
fn hat_garantierten_suchpfad(qs: &[Question]) -> bool {
    qs.iter()
        .filter(|q| !REQUIRED.contains(&q.id.as_str()))
        .any(|q| q.options.iter().any(|o| !o.categories.is_empty() || !o.queries.is_empty()))
}

fn ergaenze_garantierten_suchpfad(qs: &mut Vec<Question>, standard: &[Question]) {
    if hat_garantierten_suchpfad(qs) || qs.iter().any(|q| q.id == "ziel") {
        return;
    }
    if let Some(ziel) = standard.iter().find(|q| q.id == "ziel") {
        // Direkt hinter den technischen Pflichtfragen -- sie bestimmt
        // alles Weitere und gehoert nicht ans Ende.
        let stelle = qs.len().min(REQUIRED.len());
        qs.insert(stelle, ziel.clone());
    }
}

/// Stellt sicher, dass die technisch nötigen Fragen vorhanden sind.
///
/// Ein Modell lässt gelegentlich eine davon weg -- ohne Version oder
/// Arbeitsspeicher liesse sich aber nichts anlegen. Fehlende werden aus
/// dem Standardkatalog ergänzt, und zwar vorn, weil sie alles Weitere
/// bestimmen.
pub fn merge_required(from_model: Vec<Question>, standard: &[Question]) -> Vec<Question> {
    let mut out: Vec<Question> = Vec::new();

    for id in REQUIRED {
        let found = from_model
            .iter()
            .find(|q| q.id == id)
            .or_else(|| standard.iter().find(|q| q.id == id));
        if let Some(q) = found {
            // Die Auswahlmöglichkeiten der technischen Fragen stammen vom
            // System, nicht vom Modell -- es kennt weder die Versionsliste
            // noch den Arbeitsspeicher des Rechners.
            let mut q = q.clone();
            if let Some(original) = standard.iter().find(|s| s.id == id) {
                q.options = original.options.clone();
            }
            out.push(q);
        }
    }

    for q in from_model {
        if !REQUIRED.contains(&q.id.as_str()) && !out.iter().any(|o| o.id == q.id) {
            out.push(q);
        }
    }

    out
}

/* ---- Plan abwägen -------------------------------------------------------- */

const PLAN_PROMPT: &str = "\
Du prüfst die Zusammenstellung für eine neue Minecraft-Instanz.

Du bekommst die Beschreibung des Nutzers, seine Antworten und die Liste der \
ausgewählten Mods. Alle genannten Mods existieren — sie stammen aus einer \
echten Suche. Wähle nicht neu aus, sondern begründe.

Antworte ausschließlich mit einem JSON-Objekt, ohne Text davor oder danach:
  rationale  Zwei bis vier deutsche Sätze: warum diese Zusammenstellung zum \
             Ziel passt, und worauf zu achten ist.
  reasons    Objekt: Projektkennung -> ein deutscher Satz, warum diese Mod \
             hier sinnvoll ist.
  warnings   Liste deutscher Sätze zu Dingen, die sich stoßen könnten oder \
             die der Nutzer wissen sollte. Leer, wenn nichts auffällt.

Erfinde keine Mods und keine Versionsnummern.";

#[derive(Debug, Deserialize)]
struct PlanReply {
    #[serde(default)]
    rationale: String,
    #[serde(default)]
    reasons: std::collections::HashMap<String, String>,
    #[serde(default)]
    warnings: Vec<String>,
}

/// Lässt das Modell den fertigen Plan begründen.
///
/// Der Plan steht bereits -- das Modell ändert ihn nicht, es erklärt ihn.
/// Damit kann der Aufruf gefahrlos ausfallen: es fehlen dann die
/// Begründungen, nicht der Plan.
pub async fn explain_plan(
    model: &str,
    session: &Session,
    plan: &mut Plan,
    events: &dyn ai::client::AiEvents,
) -> Result<bool> {
    let Some(key) = ai::keys::api_key() else {
        return Ok(false);
    };

    let mods: Vec<serde_json::Value> = plan
        .mods
        .iter()
        .map(|m| {
            serde_json::json!({
                "id": m.project_id,
                "titel": m.title,
                "herkunft": if m.from_optimization { "Optimierung" } else { "Auswahl" },
            })
        })
        .collect();

    let user = serde_json::json!({
        "beschreibung": session.goal,
        "antworten": session.answer_pairs(),
        "minecraft": plan.mc_version,
        "loader": plan.loader,
        "arbeitsspeicher_mb": plan.max_ram,
        "optimierung": plan.optimization,
        "mods": mods,
    });

    let raw =
        ai::client::ask_json_streaming(&key, model, PLAN_PROMPT, &user.to_string(), &[], "plan", events)
            .await?;
    let reply: PlanReply = serde_json::from_str(&raw)
        .map_err(|e| crate::Error::other(format!("Antwort des Modells unlesbar: {e}")))?;

    if !reply.rationale.trim().is_empty() {
        plan.rationale = Some(reply.rationale);
    }
    for m in plan.mods.iter_mut() {
        if let Some(reason) = reply.reasons.get(&m.project_id) {
            if !reason.trim().is_empty() {
                m.reason = reason.clone();
            }
        }
    }
    plan.notes.extend(reply.warnings);
    Ok(true)
}

/// Ordnet Kandidaten den Antworten zu -- ohne Modell.
///
/// Wird gebraucht, wenn die Oberfläche zeigen will, welcher Vorschlag zu
/// welcher Frage gehört.
pub fn group_by_answer<'a>(
    candidates: &'a [Candidate],
    query: &str,
) -> Vec<&'a Candidate> {
    candidates
        .iter()
        .filter(|c| c.matched.iter().any(|m| m == query))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frage(id: &str) -> Question {
        Question {
            id: id.into(),
            prompt: format!("Frage {id}"),
            why: "weil".into(),
            multi: false,
            options: vec![questions::Option_ {
                id: "a".into(), label: "A".into(), detail: String::new(),
                categories: vec![], queries: vec![], project_type: None,
            }],
        }
    }

    #[test]
    fn technische_fragen_stehen_vorn_und_vollstaendig() {
        // Das Modell hat version und ram vergessen.
        let vom_modell = vec![frage("loader"), frage("geraet"), frage("eigene")];
        let standard = vec![
            frage("version"), frage("loader"), frage("geraet"), frage("ram"),
        ];

        let out = merge_required(vom_modell, &standard);
        let ids: Vec<&str> = out.iter().map(|q| q.id.as_str()).collect();

        assert_eq!(&ids[..4], &["version", "loader", "geraet", "ram"]);
        assert!(ids.contains(&"eigene"), "eigene Fragen bleiben erhalten");
    }

    #[test]
    fn auswahl_der_technischen_fragen_kommt_vom_system() {
        // Das Modell erfindet Versionen -- die des Systems gelten.
        let mut erfunden = frage("version");
        erfunden.options = vec![questions::Option_ {
            id: "1.99".into(), label: "1.99".into(), detail: String::new(),
            categories: vec![], queries: vec![], project_type: None,
        }];

        let mut echt = frage("version");
        echt.options = vec![questions::Option_ {
            id: "1.21.4".into(), label: "1.21.4".into(), detail: String::new(),
            categories: vec![], queries: vec![], project_type: None,
        }];

        let out = merge_required(vec![erfunden], &[echt]);
        assert_eq!(out[0].options[0].id, "1.21.4");
    }

    #[test]
    fn doppelte_fragen_bleiben_einmal() {
        let out = merge_required(vec![frage("loader"), frage("loader")], &[frage("loader")]);
        assert_eq!(out.iter().filter(|q| q.id == "loader").count(), 1);
    }

    #[test]
    fn leere_modellantwort_ergibt_den_standardkatalog() {
        let standard = vec![frage("version"), frage("loader"), frage("geraet"), frage("ram")];
        let out = merge_required(vec![], &standard);
        assert_eq!(out.len(), 4);
    }

    /// Zweite Sicherungsschicht (nach dem Prompt, vor der harten Garantie
    /// in `evaluate_in`): traegt keine nicht-technische Frage einen
    /// Suchbegriff, greift die deterministische Reparatur.
    fn frage_mit_suchbegriff(id: &str) -> Question {
        Question {
            options: vec![questions::Option_ {
                id: "a".into(), label: "A".into(), detail: String::new(),
                categories: vec![], queries: vec!["sodium".into()], project_type: None,
            }],
            ..frage(id)
        }
    }

    #[test]
    fn nur_technische_fragen_haben_keinen_garantierten_suchpfad() {
        let qs = vec![frage("version"), frage("loader"), frage("geraet"), frage("ram")];
        assert!(!hat_garantierten_suchpfad(&qs));
    }

    #[test]
    fn eine_inhaltstragende_frage_reicht_als_suchpfad() {
        let qs = vec![frage("version"), frage_mit_suchbegriff("ziel")];
        assert!(hat_garantierten_suchpfad(&qs));
    }

    #[test]
    fn fehlender_suchpfad_bekommt_ziel_frage_angehaengt() {
        // Exakt der Vorfall: das Modell ersetzte "ziel" durch eine rein
        // technische "leistungsziel"-Frage ohne jeden Suchbegriff.
        let mut vom_modell = vec![
            frage("version"), frage("loader"), frage("geraet"), frage("ram"),
            frage("leistungsziel"),
        ];
        let standard = vec![
            frage("version"), frage("loader"), frage("geraet"), frage("ram"),
            frage_mit_suchbegriff("ziel"),
        ];

        assert!(!hat_garantierten_suchpfad(&vom_modell), "Testaufbau: sollte leer starten");
        ergaenze_garantierten_suchpfad(&mut vom_modell, &standard);

        assert!(vom_modell.iter().any(|q| q.id == "ziel"), "{:?}",
                vom_modell.iter().map(|q| &q.id).collect::<Vec<_>>());
        assert!(hat_garantierten_suchpfad(&vom_modell));
    }

    #[test]
    fn vorhandener_suchpfad_wird_nicht_doppelt_ergaenzt() {
        let mut vom_modell = vec![frage("version"), frage_mit_suchbegriff("eigene_frage")];
        let standard = vec![frage("version"), frage_mit_suchbegriff("ziel")];

        ergaenze_garantierten_suchpfad(&mut vom_modell, &standard);

        assert!(!vom_modell.iter().any(|q| q.id == "ziel"),
                "ein bereits vorhandener Suchpfad braucht keine Ergänzung");
    }

    #[test]
    fn eigene_ziel_frage_wird_nicht_ueberschrieben() {
        // Das Modell hat eine eigene "ziel"-Frage geliefert, nur ohne
        // Suchbegriffe -- die darf nicht durch die Standardversion
        // ersetzt werden, sonst gehen die Formulierungen des Modells
        // verloren. Die harte Garantie in evaluate_in greift dann eben
        // spaeter.
        let mut vom_modell = vec![frage("version"), frage("ziel")];
        let standard = vec![frage("version"), frage_mit_suchbegriff("ziel")];

        ergaenze_garantierten_suchpfad(&mut vom_modell, &standard);

        assert_eq!(vom_modell.iter().filter(|q| q.id == "ziel").count(), 1);
        assert!(vom_modell.iter().find(|q| q.id == "ziel").unwrap().options[0].queries.is_empty(),
                "die eigene (leere) Fassung des Modells bleibt unangetastet");
    }
}
