//! Anfragen ausbremsen, bevor der Dienst es tut -- je Modell, nach dem,
//! was er selbst meldet.
//!
//! Mistral begrenzt nach Tarif **und** Modell, in zwei unabhaengigen
//! Dimensionen: Anfragen je Sekunde und Token je Minute. Die genauen Werte
//! stehen nur im Kontobereich, nicht im Katalog -- eine fest eingetragene
//! Zahl waere also von vornherein eine Schaetzung und spaetestens beim
//! naechsten Tarifwechsel falsch.
//!
//! Stattdessen wird gelernt: jede Antwort traegt `X-RateLimit-…`-Koepfe,
//! daraus wird der Abstand je Modell abgeleitet. Bis die erste Antwort da
//! ist, gilt ein vorsichtiger Standardwert.
//!
//! Auf ein 429 erst zu reagieren, waere die schlechtere Haelfte: die
//! Anfrage ist dann schon verbraucht.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

/// Bis etwas Besseres bekannt ist.
///
/// Der freie Zugang liegt in der Groessenordnung einer Anfrage je Sekunde;
/// etwas mehr Abstand kostet kaum Zeit und erspart das Zurueckweichen.
const DEFAULT_INTERVAL: Duration = Duration::from_millis(1200);

/// Nie schneller als das, auch wenn der Dienst mehr erlaubt.
const MIN_INTERVAL: Duration = Duration::from_millis(120);

/// Nie langsamer als das -- sonst wirkt die Oberflaeche eingefroren.
const MAX_INTERVAL: Duration = Duration::from_secs(12);

/// Ab welchem Restkontingent vorsorglich gebremst wird.
///
/// Die letzten Anfragen vor der Grenze zu verheizen, bringt nichts: dann
/// steht man vor einer Wartezeit statt vor einem langsameren Fortgang.
const SLOW_DOWN_BELOW: f64 = 0.15;

#[derive(Debug, Clone, Serialize)]
pub struct Budget {
    pub model: String,
    /// Abstand, der derzeit eingehalten wird.
    pub interval_ms: u64,
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    /// Woher die Werte stammen: `gemeldet` oder `voreinstellung`.
    pub source: &'static str,
}

#[derive(Debug, Clone)]
struct State {
    interval: Duration,
    next_free: Option<Instant>,
    limit: Option<u64>,
    remaining: Option<u64>,
    learned: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            interval: DEFAULT_INTERVAL,
            next_free: None,
            limit: None,
            remaining: None,
            learned: false,
        }
    }
}

fn states() -> &'static Mutex<HashMap<String, State>> {
    static S: std::sync::OnceLock<Mutex<HashMap<String, State>>> = std::sync::OnceLock::new();
    S.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Wartet, bis fuer dieses Modell die naechste Anfrage dran ist.
pub async fn wait_turn(model: &str) {
    let sleep_for = {
        let Ok(mut map) = states().lock() else { return };
        let state = map.entry(model.to_string()).or_default();

        let now = Instant::now();
        let wait = match state.next_free {
            Some(t) if t > now => t - now,
            _ => Duration::ZERO,
        };

        // Den Platz sofort vormerken, damit gleichzeitige Aufrufe sich
        // einreihen statt alle denselben freien Platz zu sehen.
        state.next_free = Some(now + wait + state.interval);
        wait
    };

    if !sleep_for.is_zero() {
        tokio::time::sleep(sleep_for).await;
    }
}

/// Nimmt auf, was die Antwort ueber die Grenzen verraet.
pub fn observe(model: &str, headers: &reqwest::header::HeaderMap) {
    let reading = read_headers(headers);
    let Ok(mut map) = states().lock() else { return };
    let state = map.entry(model.to_string()).or_default();

    if let Some(limit) = reading.limit {
        state.limit = Some(limit);
    }
    if let Some(remaining) = reading.remaining {
        state.remaining = Some(remaining);
    }

    if let Some(interval) = derive_interval(&reading, state.limit, state.remaining) {
        state.interval = interval.clamp(MIN_INTERVAL, MAX_INTERVAL);
        state.learned = true;
    }
}

/// Was aus den Koepfen gelesen werden konnte.
#[derive(Debug, Default, PartialEq)]
pub struct Reading {
    pub limit: Option<u64>,
    pub remaining: Option<u64>,
    /// Sekunden bis zur Erneuerung des Kontingents.
    pub reset_seconds: Option<u64>,
}

/// Liest die `x-ratelimit-…`-Koepfe.
///
/// Die Benennung ist zwischen Diensten und Fassungen nicht einheitlich --
/// mal `x-ratelimit-remaining`, mal mit angehaengter Dimension wie
/// `-requests`. Deshalb wird nicht auf genaue Namen geprueft, sondern auf
/// Bestandteile. Token-bezogene Koepfe werden dabei ausgelassen: sie zaehlen
/// eine andere Groesse und lassen sich nicht in einen Anfrageabstand
/// umrechnen.
pub fn read_headers(headers: &reqwest::header::HeaderMap) -> Reading {
    let mut out = Reading::default();

    for (name, value) in headers.iter() {
        let key = name.as_str().to_ascii_lowercase();
        if !key.contains("ratelimit") {
            continue;
        }
        if key.contains("token") {
            continue;
        }
        let Ok(text) = value.to_str() else { continue };
        let Ok(number) = text.trim().parse::<u64>() else { continue };

        if key.contains("remaining") {
            out.remaining = Some(number);
        } else if key.contains("reset") {
            out.reset_seconds = Some(number);
        } else if key.contains("limit") {
            out.limit = Some(number);
        }
    }

    out
}

/// Leitet aus den Angaben einen Abstand ab.
///
/// Sind Rest und Erneuerungszeitpunkt bekannt, wird der verbleibende
/// Zeitraum gleichmaessig auf die verbleibenden Anfragen verteilt. Das ist
/// die ehrlichste Rechnung: sie nutzt das Kontingent aus, ohne es zu
/// ueberziehen.
fn derive_interval(reading: &Reading, limit: Option<u64>, remaining: Option<u64>) -> Option<Duration> {
    let remaining = remaining.or(reading.remaining)?;

    if let Some(reset) = reading.reset_seconds.filter(|r| *r > 0) {
        if remaining == 0 {
            // Nichts mehr frei: bis zur Erneuerung warten.
            return Some(Duration::from_secs(reset));
        }
        return Some(Duration::from_secs_f64(reset as f64 / remaining as f64));
    }

    // Ohne Erneuerungszeitpunkt bleibt der Anteil: wird es knapp, bremsen.
    let limit = limit.or(reading.limit).filter(|l| *l > 0)?;
    let share = remaining as f64 / limit as f64;
    if share < SLOW_DOWN_BELOW {
        return Some(DEFAULT_INTERVAL * 3);
    }
    None
}

/// Liest `Retry-After`, wenn der Dienst einen schickt.
///
/// Das ist die verlaesslichste Angabe -- nur der Dienst weiss, wann die
/// Grenze wieder offen ist.
pub fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    let value = headers.get(reqwest::header::RETRY_AFTER)?.to_str().ok()?;
    let seconds: u64 = value.trim().parse().ok()?;
    (seconds <= 120).then(|| Duration::from_secs(seconds))
}

/// Was derzeit ueber ein Modell bekannt ist -- fuer die Anzeige.
pub fn budget(model: &str) -> Budget {
    let state = states()
        .lock()
        .ok()
        .and_then(|m| m.get(model).cloned())
        .unwrap_or_default();

    Budget {
        model: model.to_string(),
        interval_ms: state.interval.as_millis() as u64,
        limit: state.limit,
        remaining: state.remaining,
        source: if state.learned { "gemeldet" } else { "voreinstellung" },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderName, HeaderValue, RETRY_AFTER};

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut h = HeaderMap::new();
        for (k, v) in pairs {
            h.insert(
                HeaderName::from_bytes(k.as_bytes()).unwrap(),
                HeaderValue::from_str(v).unwrap(),
            );
        }
        h
    }

    #[test]
    fn koepfe_werden_gelesen() {
        let h = headers(&[
            ("x-ratelimit-limit", "100"),
            ("x-ratelimit-remaining", "42"),
            ("x-ratelimit-reset", "30"),
        ]);
        let r = read_headers(&h);
        assert_eq!(r.limit, Some(100));
        assert_eq!(r.remaining, Some(42));
        assert_eq!(r.reset_seconds, Some(30));
    }

    #[test]
    fn dimension_im_namen_stoert_nicht() {
        // Manche Dienste haengen die Dimension an.
        let h = headers(&[("x-ratelimit-remaining-requests", "7")]);
        assert_eq!(read_headers(&h).remaining, Some(7));
    }

    #[test]
    fn token_koepfe_werden_ausgelassen() {
        // Token zaehlen eine andere Groesse und taugen nicht fuer den
        // Anfrageabstand.
        let h = headers(&[
            ("x-ratelimit-remaining-tokens", "500000"),
            ("x-ratelimit-remaining-requests", "3"),
        ]);
        assert_eq!(read_headers(&h).remaining, Some(3));
    }

    #[test]
    fn fremde_koepfe_werden_ignoriert() {
        let h = headers(&[("content-type", "application/json"), ("x-request-id", "abc")]);
        assert_eq!(read_headers(&h), Reading::default());
    }

    #[test]
    fn rest_und_erneuerung_ergeben_den_abstand() {
        // 30 Sekunden, 10 Anfragen frei -> alle drei Sekunden eine.
        let r = Reading { limit: Some(100), remaining: Some(10), reset_seconds: Some(30) };
        assert_eq!(derive_interval(&r, Some(100), Some(10)), Some(Duration::from_secs(3)));
    }

    #[test]
    fn nichts_mehr_frei_heisst_bis_zur_erneuerung_warten() {
        let r = Reading { limit: Some(100), remaining: Some(0), reset_seconds: Some(20) };
        assert_eq!(derive_interval(&r, Some(100), Some(0)), Some(Duration::from_secs(20)));
    }

    #[test]
    fn knappes_kontingent_bremst_auch_ohne_erneuerungszeit() {
        // 8 von 100 uebrig, kein reset-Kopf -> vorsorglich langsamer.
        let r = Reading { limit: Some(100), remaining: Some(8), reset_seconds: None };
        let d = derive_interval(&r, Some(100), Some(8)).unwrap();
        assert!(d > DEFAULT_INTERVAL);
    }

    #[test]
    fn reichliches_kontingent_aendert_nichts() {
        let r = Reading { limit: Some(100), remaining: Some(90), reset_seconds: None };
        assert_eq!(derive_interval(&r, Some(100), Some(90)), None);
    }

    #[test]
    fn ohne_angaben_keine_ableitung() {
        assert_eq!(derive_interval(&Reading::default(), None, None), None);
    }

    #[test]
    fn abstand_wird_je_modell_getrennt_gefuehrt() {
        observe("modell-a", &headers(&[
            ("x-ratelimit-remaining", "2"),
            ("x-ratelimit-reset", "60"),
        ]));
        let a = budget("modell-a");
        let b = budget("modell-b");
        assert_eq!(a.source, "gemeldet");
        assert_eq!(b.source, "voreinstellung", "ein anderes Modell bleibt unberührt");
        assert!(a.interval_ms > b.interval_ms);
    }

    #[test]
    fn abstand_bleibt_in_vernuenftigen_grenzen() {
        // Absurd knappe Angabe -> gedeckelt, nicht minutenlang.
        observe("modell-c", &headers(&[
            ("x-ratelimit-remaining", "1"),
            ("x-ratelimit-reset", "9999"),
        ]));
        assert_eq!(budget("modell-c").interval_ms, MAX_INTERVAL.as_millis() as u64);
    }

    #[test]
    fn retry_after_in_sekunden() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_static("5"));
        assert_eq!(retry_after(&h), Some(Duration::from_secs(5)));
        assert_eq!(retry_after(&HeaderMap::new()), None);
    }

    #[test]
    fn unplausibel_lange_wartezeit_wird_verworfen() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, HeaderValue::from_static("3600"));
        assert_eq!(retry_after(&h), None);
    }
}
