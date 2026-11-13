//! Die Startseite: Neuigkeiten und was davon dich betrifft.
//!
//! Zwei Teile mit unterschiedlichem Wert:
//!
//! * **Neuigkeiten** -- Ueberschrift, die vom Feed gelieferte Kurzfassung
//!   und ein Verweis auf die Quelle. Der Artikeltext bleibt dort, wo er
//!   hingehoert.
//! * **Fuer dich** -- aus denselben Daten abgeleitet, aber auf die
//!   vorhandenen Instanzen bezogen. Das ist der Teil, der etwas wert ist:
//!   "Fabric gibt es jetzt fuer 26.3, du hast zwei 26er-Instanzen" statt
//!   "irgendwo wurde etwas veroeffentlicht".

pub mod feed;

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::http;

pub use feed::NewsItem;

/// Woher die Neuigkeiten kommen.
struct Source {
    name: &'static str,
    url: &'static str,
}

const SOURCES: &[Source] = &[
    Source { name: "Minecraft.net", url: "https://www.minecraft.net/en-us/feeds/community-content/rss" },
    Source { name: "Fabric",        url: "https://fabricmc.net/feed.xml" },
];

/// Wie lange ein Abruf gilt.
///
/// Diese Feeds erscheinen taeglich bis woechentlich. Sie bei jedem
/// Aufruf der Startseite neu zu holen waere Last ohne Gegenwert -- fuer
/// uns wie fuer die Quellen.
const CACHE_TTL: Duration = Duration::from_secs(30 * 60);

struct Cache {
    items: Vec<NewsItem>,
    fetched: Instant,
}

fn cache() -> &'static Mutex<Option<Cache>> {
    static C: std::sync::OnceLock<Mutex<Option<Cache>>> = std::sync::OnceLock::new();
    C.get_or_init(|| Mutex::new(None))
}

/// Holt die Neuigkeiten aller Quellen.
///
/// Eine ausgefallene Quelle laesst die uebrigen unberuehrt: eine
/// Startseite, die wegen eines nicht erreichbaren Feeds leer bleibt,
/// waere schlechter als eine mit weniger Eintraegen.
pub async fn headlines(force: bool) -> Vec<NewsItem> {
    if !force {
        if let Ok(guard) = cache().lock() {
            if let Some(c) = guard.as_ref() {
                if c.fetched.elapsed() < CACHE_TTL {
                    return c.items.clone();
                }
            }
        }
    }

    let mut items = Vec::new();
    for source in SOURCES {
        match http::client().get(source.url).send().await {
            Ok(response) => match response.error_for_status() {
                Ok(ok) => match ok.bytes().await {
                    Ok(raw) => {
                        let text = feed::decode(&raw);
                        items.extend(feed::parse(&text, source.name));
                    }
                    Err(e) => eprintln!("[news] {} unlesbar: {e}", source.name),
                },
                Err(e) => eprintln!("[news] {} antwortet nicht: {e}", source.name),
            },
            Err(e) => eprintln!("[news] {} nicht erreichbar: {e}", source.name),
        }
    }

    // Je Quelle die neuesten, damit eine ergiebige Quelle die andere
    // nicht verdraengt.
    let mut mixed: Vec<NewsItem> = Vec::new();
    for source in SOURCES {
        mixed.extend(items.iter().filter(|i| i.source == source.name).take(6).cloned());
    }

    if let Ok(mut guard) = cache().lock() {
        *guard = Some(Cache { items: mixed.clone(), fetched: Instant::now() });
    }

    mixed
}

/* ---- Fuer dich --------------------------------------------------------- */

/// Ein Hinweis, der sich auf die vorhandenen Instanzen bezieht.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    /// `version`, `mod_update`, `crash`, `loader`
    pub kind: String,
    pub title: String,
    pub detail: String,
    /// Betroffene Instanz, sofern eindeutig.
    pub instance_id: Option<String>,
    /// Wie dringend: 0 beilaeufig, 2 sollte man ansehen.
    pub weight: u8,
}

/// Leitet aus Instanzen und Manifest ab, was den Nutzer betrifft.
pub async fn insights() -> Result<Vec<Insight>> {
    let instances = crate::instance::list()?;
    if instances.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();

    // 1. Neuere Minecraft-Versionen als die installierten.
    if let Ok(versions) = crate::manifest::list_versions().await {
        let newest_release = versions
            .iter()
            .find(|v| matches!(v.channel, crate::manifest::VersionChannel::Release));

        if let Some(newest) = newest_release {
            let uses_it = instances.iter().any(|i| i.mc_version == newest.id);
            if !uses_it {
                out.push(Insight {
                    kind: "version".into(),
                    title: format!("Minecraft {} ist da", newest.id),
                    detail: format!(
                        "Keine deiner {} Instanzen nutzt diese Version.",
                        instances.len()
                    ),
                    instance_id: None,
                    weight: 1,
                });
            }
        }
    }

    // 2. Abstuerze, die noch niemand angesehen hat.
    for instance in &instances {
        if let Ok(Some(d)) = crate::launch::analyze_instance(&instance.id) {
            out.push(Insight {
                kind: "crash".into(),
                title: format!("{} ist abgestürzt", instance.name),
                detail: d.cause,
                instance_id: Some(instance.id.clone()),
                weight: 2,
            });
        }
    }

    // Dringendes zuerst.
    out.sort_by(|a, b| b.weight.cmp(&a.weight));
    Ok(out)
}

/// Was die Startseite insgesamt zeigt.
#[derive(Debug, Clone, Serialize)]
pub struct Dashboard {
    pub headlines: Vec<NewsItem>,
    pub insights: Vec<Insight>,
    /// Zuletzt gespielte Instanz, fuer den Fortsetzen-Knopf.
    pub resume: Option<crate::instance::InstanceConfig>,
    pub total_playtime: u64,
    pub instance_count: usize,
}

pub async fn dashboard(force: bool) -> Result<Dashboard> {
    let instances = crate::instance::list()?;

    let resume = instances
        .iter()
        .filter(|i| !i.last_played.is_empty())
        .max_by(|a, b| a.last_played.cmp(&b.last_played))
        .cloned();

    Ok(Dashboard {
        headlines: headlines(force).await,
        insights: insights().await.unwrap_or_default(),
        total_playtime: instances.iter().map(|i| i.play_seconds).sum(),
        instance_count: instances.len(),
        resume,
    })
}
