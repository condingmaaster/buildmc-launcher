//! Kandidaten zu den Antworten suchen.
//!
//! Die Suchbegriffe stehen nach der Auswertung fest -- hier werden sie
//! gegen Modrinth geschickt. Bewusst **ohne** Sprachmodell: das ist eine
//! Zuordnung, keine Deutung, und jede Modellanfrage hier wäre eine, die
//! später am Ratenlimit fehlt.
//!
//! Zwei Dinge sind dabei wichtig:
//!
//! * **Gleichzeitig statt nacheinander.** Ein Dutzend Anfragen der Reihe
//!   nach dauert spürbar; nebeneinander sind sie in der Zeit der langsamsten
//!   erledigt.
//! * **Entdoppeln.** „performance" und „optimization" liefern weitgehend
//!   dieselben Treffer. Ohne Zusammenführung stünde Sodium dreimal in der
//!   Auswahl.

use std::collections::HashMap;

use futures::stream::{FuturesUnordered, StreamExt};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::modrinth::{self, SearchResult};

/// Ein Vorschlag, wie ihn die Oberfläche zeigt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    #[serde(flatten)]
    pub project: SearchResult,
    /// Zu welchen Antworten dieser Treffer gehört.
    pub matched: Vec<String>,
    /// Vorausgewählt? Die offensichtlichen Treffer sollen nicht erst
    /// angeklickt werden müssen.
    pub preselected: bool,
    /// `false`, wenn dieser Treffer nur gefunden wurde, weil der
    /// Versionsfilter fallen gelassen wurde (siehe `candidates()`) -- die
    /// Mod passt dann möglicherweise nicht exakt zur gewählten Version.
    #[serde(default = "immer_wahr")]
    pub version_exact: bool,
}

fn immer_wahr() -> bool {
    true
}

/// Wie viele Treffer je Suchbegriff geholt werden.
///
/// Mehr verwässert nur: nach den ersten fünf kommen bei einem gut
/// getroffenen Begriff schon die Randfälle.
const PER_QUERY: usize = 6;

/// Wie viele Vorschläge am Ende stehenbleiben.
const MAX_CANDIDATES: usize = 24;

/// Fuehrt eine Runde Suchen gleichzeitig aus und fuehrt die Treffer
/// zusammen -- ohne jede Versions-Rueckfall-Logik, die steht in
/// `candidates()` darueber.
async fn suchrunde(
    searches: &[(String, String, Vec<String>)],
    game_version: Option<&str>,
    loader: Option<&str>,
) -> HashMap<String, (SearchResult, Vec<String>)> {
    let mut running = FuturesUnordered::new();

    for (kind, query, categories) in searches {
        let kind = kind.clone();
        let query = query.clone();
        let cats = categories.clone();
        let gv = game_version.map(str::to_string);
        let ld = loader.map(str::to_string);
        // Ohne Suchwort benennt die Kategorie den Eintrag -- sonst hiesse
        // er in der Oberfläche schlicht nichts.
        let label = if query.is_empty() { cats.join(", ") } else { query.clone() };

        running.push(async move {
            let page = modrinth::search_in(&query, gv.as_deref(), ld.as_deref(), &kind, &cats, 0)
                .await
                .ok();
            (label, page)
        });
    }

    // Kennung -> (Treffer, welche Begriffe ihn brachten)
    let mut merged: HashMap<String, (SearchResult, Vec<String>)> = HashMap::new();
    while let Some((query, page)) = running.next().await {
        let Some(page) = page else { continue };
        for hit in page.results.into_iter().take(PER_QUERY) {
            merged
                .entry(hit.project_id.clone())
                .and_modify(|(_, matched)| matched.push(query.clone()))
                .or_insert_with(|| (hit, vec![query.clone()]));
        }
    }
    merged
}

/// Sucht alle Begriffe gleichzeitig und führt die Treffer zusammen.
///
/// Kommt die erste Runde mit gesetztem Versionsfilter leer zurück, folgt
/// ein zweiter Versuch ohne ihn -- nur mit dem Lader. Das faengt exotische
/// oder sehr neue Versionen ab, die Modrinth noch nicht durchgaengig
/// indiziert hat: lieber Treffer zeigen, die "wahrscheinlich passen und
/// als solche gekennzeichnet sind", als eine leere Liste, obwohl es die
/// Mods durchaus gibt.
pub async fn candidates(
    searches: &[(String, String, Vec<String>)],
    game_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<Candidate>> {
    let mut merged = suchrunde(searches, game_version, loader).await;
    let mut version_exact = true;

    if merged.is_empty() && game_version.is_some() {
        merged = suchrunde(searches, None, loader).await;
        version_exact = false;
    }

    let mut out: Vec<Candidate> = merged
        .into_values()
        .map(|(project, matched)| Candidate {
            // Von mehreren Begriffen getroffen zu werden, ist ein starkes
            // Zeichen: die Mod liegt dann mitten im gesuchten Feld.
            preselected: matched.len() > 1 || project.downloads > 20_000_000,
            project,
            matched,
            version_exact,
        })
        .collect();

    out.sort_by(|a, b| rank(b).partial_cmp(&rank(a)).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(MAX_CANDIDATES);
    Ok(out)
}

/// Wie gut passt ein Treffer?
///
/// Beliebtheit allein wäre irreführend -- sie bevorzugt immer dieselben
/// grossen Namen. Deshalb zählt vor allem, von wie vielen Begriffen ein
/// Treffer gefunden wurde; die Downloadzahl entscheidet nur bei Gleichstand
/// und wird logarithmisch gedämpft, damit eine Mod mit 200 Millionen nicht
/// alles überdeckt.
fn rank(c: &Candidate) -> f64 {
    let breite = c.matched.len() as f64 * 10.0;
    let beliebtheit = (c.project.downloads.max(1) as f64).log10();
    breite + beliebtheit
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(id: &str, downloads: u64) -> SearchResult {
        SearchResult {
            provider: "modrinth".into(),
            project_id: id.into(),
            slug: id.into(),
            title: id.into(),
            description: String::new(),
            author: String::new(),
            downloads,
            follows: 0,
            icon_url: None,
            categories: vec![],
            project_type: "mod".into(),
            loaders: vec![],
            game_versions: vec![],
            updated_at: String::new(),
            license: None,
            client_side: "required".into(),
            server_side: "unsupported".into(),
        }
    }

    fn cand(id: &str, downloads: u64, matched: &[&str]) -> Candidate {
        Candidate {
            project: hit(id, downloads),
            matched: matched.iter().map(|s| s.to_string()).collect(),
            preselected: false,
            version_exact: true,
        }
    }

    #[test]
    fn breite_schlaegt_beliebtheit() {
        // Von zwei Begriffen gefunden, aber unbekannter.
        let breit = cand("a", 10_000, &["performance", "optimization"]);
        // Sehr beliebt, aber nur ein Begriff.
        let beliebt = cand("b", 200_000_000, &["performance"]);
        assert!(rank(&breit) > rank(&beliebt),
                "sonst überdecken dieselben großen Namen jede Suche");
    }

    #[test]
    fn bei_gleicher_breite_entscheidet_beliebtheit() {
        let a = cand("a", 1_000_000, &["x"]);
        let b = cand("b", 1_000, &["x"]);
        assert!(rank(&a) > rank(&b));
    }

    #[test]
    fn null_downloads_bricht_die_rechnung_nicht() {
        // log10(0) wäre minus unendlich.
        assert!(rank(&cand("a", 0, &["x"])).is_finite());
    }

    #[test]
    fn mehrfachtreffer_werden_vorausgewaehlt() {
        let c = Candidate {
            preselected: true,
            ..cand("a", 100, &["x", "y"])
        };
        assert!(c.preselected);
    }
}
