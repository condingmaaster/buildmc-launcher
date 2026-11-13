//! Anbindung an Mistral.
//!
//! Gerufen wird **`/v1/conversations`**, nicht `/v1/chat/completions`.
//! Der Unterschied ist nicht kosmetisch: die beiden Endpunkte haben
//! getrennte Kontingente. Ein Schlüssel kann auf dem einen `0` Anfragen je
//! Minute haben und auf dem anderen arbeiten -- genau das war hier der
//! Fall, und es sah wie ein gesperrtes Modell aus.
//!
//! Der Körper sieht anders aus als bei Chat-Completions:
//!
//! * Der Systemauftrag heisst `instructions`, nicht eine Nachricht mit
//!   Rolle `system`.
//! * Die Nachrichten heissen `inputs`, nicht `messages`.
//! * `temperature`, `max_tokens` und **`reasoning_effort`** stehen in
//!   `completion_args`, nicht auf der obersten Ebene.
//!
//! Die Antwort kommt als Liste von Ausgaben mit Inhaltsblöcken. Neben dem
//! Text steht dort auch der Gedankengang als eigener Block -- der wird
//! übersprungen, er gehört nicht in die Auswertung.

use std::collections::HashMap;
use std::time::Duration;

use futures::StreamExt;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http;
use crate::launch::diagnose::Diagnosis;

const CONVERSATIONS_URL: &str = "https://api.mistral.ai/v1/conversations";
const MODELS_URL: &str = "https://api.mistral.ai/v1/models";

/// Voreinstellung. Die Kennung stammt aus einem Aufruf, der nachweislich
/// durchging -- nicht aus einem Modellkatalog.
pub const DEFAULT_MODEL: &str = "mistral-medium-latest";

/// Ersatz, wenn das erste Modell nicht erreichbar ist.
///
/// Es wird **nie von allein** genommen: ein schwächeres Modell liefert
/// schlechtere Vorschläge, und das darf nicht unbemerkt geschehen.
pub const FALLBACK_MODEL: &str = "ministral-14b-latest";

const MAX_RETRIES: u32 = 6;
const BASE_DELAY_MS: u64 = 2000;
/// Obergrenze je Wartezeit -- sonst wirkt der letzte Versuch wie ein Hänger.
const MAX_DELAY_MS: u64 = 20_000;

/// Wie viele Protokollzeilen ans Modell gehen.
const TAIL_LINES: usize = 220;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Deserialize)]
struct ModelsResponse {
    data: Vec<ModelInfo>,
}

/* ---- Antwortformat ------------------------------------------------------ */

#[derive(Debug, Deserialize)]
struct ConversationResponse {
    #[serde(default)]
    outputs: Vec<Output>,
    usage: Option<Usage>,
}

#[derive(Debug, Deserialize)]
struct Usage {
    completion_tokens: u64,
}

#[derive(Debug, Deserialize)]
struct Output {
    #[serde(default)]
    content: ContentField,
}

/// Der Inhalt ist mal eine Zeichenkette, mal eine Liste von Blöcken.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ContentField {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl Default for ContentField {
    fn default() -> Self {
        ContentField::Blocks(Vec::new())
    }
}

#[derive(Debug, Deserialize)]
struct ContentBlock {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    text: String,
}

/// Zieht den Antworttext heraus.
///
/// Der Gedankengang steht als eigener Block vom Typ `thinking` daneben und
/// wird übersprungen -- er ist für den Nutzer interessant, aber nicht die
/// Antwort, und in einer JSON-Auswertung wäre er schlicht Störrauschen.
fn extract_text(response: &ConversationResponse) -> String {
    let mut out = String::new();
    for output in &response.outputs {
        match &output.content {
            ContentField::Text(t) => out.push_str(t),
            ContentField::Blocks(blocks) => {
                for b in blocks {
                    if b.kind == "text" {
                        out.push_str(&b.text);
                    }
                }
            }
        }
    }
    out
}

/* ---- Modelle ------------------------------------------------------------ */

/// Taugt dieses Modell fuer den Launcher?
///
/// Zwei Bedingungen, und beide sind noetig:
///
/// 1. Aus der **Mistral**- oder **Ministral**-Reihe. Codestral, Pixtral,
///    Devstral und Voxtral verfolgen andere Zwecke.
/// 2. **Kein Sonderzweck.** `mistral-embed`, `mistral-ocr-latest` und
///    `mistral-moderation-latest` tragen zwar das Praefix, koennen aber
///    nicht chatten. Sie in der Auswahl zu lassen hiesse, den Nutzer in
///    einen Fehler laufen zu lassen, den er nicht einordnen kann.
pub fn usable_model(id: &str) -> bool {
    let id = id.to_ascii_lowercase();

    if !(id.starts_with("mistral-") || id.starts_with("ministral-")) {
        return false;
    }

    const SONDERZWECKE: [&str; 7] = [
        "embed", "ocr", "moderation", "transcribe", "tts", "voice", "rerank",
    ];
    !SONDERZWECKE.iter().any(|z| id.contains(z))
}

pub async fn list_models(api_key: &str) -> Result<Vec<ModelInfo>> {
    let response = http::client()
        .get(MODELS_URL)
        .bearer_auth(api_key)
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::other("Der Schlüssel wurde abgelehnt"));
    }
    let parsed: ModelsResponse = response.error_for_status()?.json().await?;

    let mut models: Vec<ModelInfo> =
        parsed.data.into_iter().filter(|m| usable_model(&m.id)).collect();

    // Die grosse Reihe zuerst, danach alphabetisch.
    models.sort_by(|a, b| {
        let rank = |id: &str| if id.starts_with("mistral-") { 0 } else { 1 };
        rank(&a.id).cmp(&rank(&b.id)).then_with(|| a.id.cmp(&b.id))
    });
    Ok(models)
}

/* ---- Eingebaute Werkzeuge ----------------------------------------------- */

/// Werkzeuge, die der Conversations-Endpunkt selbst mitbringt.
///
/// Der Launcher nutzt sie gezielt, nicht dauerhaft: bei einem Absturz,
/// den keine Regel abdeckt. Bei den erkannten Fehlerbildern steht die
/// Antwort ohnehin fest, und die Werkzeuge kosteten dort nur Zeit und
/// Kontingent.
///
/// Zwei sind dabei sinnvoll:
///
/// * **Websuche** -- eine unbekannte Absturzsignatur steht fast immer
///   irgendwo in einem Fehlerbericht.
/// * **Codeausführung** -- eine Stapelspur systematisch zu zerlegen ist
///   Codearbeit: Rahmen zählen, Paketpräfixe gruppieren, `Caused by`-Ketten
///   verfolgen. Auch Fabrics Versionsbereiche (`>=1.2.0 <2.0.0`) lassen
///   sich nicht durch Zeichenkettenvergleich prüfen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    WebSearch,
    WebSearchPremium,
    CodeInterpreter,
}

impl Tool {
    fn key(self) -> &'static str {
        match self {
            Tool::WebSearch => "web_search",
            Tool::WebSearchPremium => "web_search_premium",
            Tool::CodeInterpreter => "code_interpreter",
        }
    }
}

/// Bringt die Werkzeugliste in eine zulässige Form.
///
/// Die beiden Suchwerkzeuge schließen einander aus -- werden beide
/// gesendet, lehnt die Schnittstelle ab. Die bezahlte Fassung gewinnt,
/// weil sie die gründlichere ist.
fn sanitize_tools(mut tools: Vec<Tool>) -> Vec<Tool> {
    let hat_premium = tools.contains(&Tool::WebSearchPremium);
    if hat_premium {
        tools.retain(|t| *t != Tool::WebSearch);
    }
    tools.dedup();
    tools
}

/* ---- Anfrage ------------------------------------------------------------ */

/// Baut einen Anfragekörper im Format des Conversations-Endpunkts.
fn build_body(
    model: &str,
    instructions: &str,
    user: &str,
    reasoning: bool,
    tools: &[Tool],
) -> serde_json::Value {
    let tools = sanitize_tools(tools.to_vec());

    // Werkzeuge -- vor allem Websuche -- haengen ihre Treffer als Teil der
    // Antwort an, und das war im echten Test mehrere Kilobyte. Beim
    // Grundbudget von 4096 waere davon kaum noch Platz fuer den eigentlichen
    // Text uebrig, selbst ohne Denkschritt.
    let max_tokens: u64 = if tools.is_empty() { 4096 } else { 8192 };

    let mut args = serde_json::json!({
        "temperature": 0.2,
        "max_tokens": max_tokens,
        "top_p": 1,
    });

    // `reasoning_effort` gehört hier hinein, nicht auf die oberste Ebene.
    if reasoning && super::capabilities::allowed(model, super::capabilities::Extra::Reasoning) {
        args["reasoning_effort"] = serde_json::json!("high");
    }

    let mut body = serde_json::json!({
        "model": model,
        "instructions": instructions,
        "inputs": [{ "role": "user", "content": user }],
        "completion_args": args,
    });

    if !tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            tools
                .into_iter()
                .map(|t| serde_json::json!({ "type": t.key() }))
                .collect(),
        );
    }

    body
}

/// Stellt eine Frage und erwartet JSON zurück.
///
/// Es wird bewusst **kein** `response_format` gesetzt: der Auftrag verlangt
/// JSON im Klartext, und `strip_fence` räumt einen Code-Zaun weg, falls das
/// Modell doch einen setzt. Ein Parameter weniger ist ein Fehlschlag weniger.
pub async fn ask_json(api_key: &str, model: &str, system: &str, user: &str) -> Result<String> {
    ask_json_with(api_key, model, system, user, &[]).await
}

/// Ob Denkaufwand angefordert werden soll -- aus den Einstellungen.
/// Was dieses Modell abgelehnt hat.
pub fn capabilities_of(model: &str) -> Vec<String> {
    super::capabilities::rejections(model)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn reasoning_wanted() -> bool {
    crate::store::load_config().ai_reasoning
}

/// Wie `ask_json`, aber mit eingebauten Werkzeugen.
pub async fn ask_json_with(
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    tools: &[Tool],
) -> Result<String> {
    let body = build_body(model, system, user, reasoning_wanted(), tools);
    let raw = post(api_key, &body).await?;
    Ok(strip_fence(&raw).to_string())
}

/// Entfernt einen Code-Zaun, falls das Modell doch einen setzt.
fn strip_fence(s: &str) -> &str {
    let t = s.trim();
    let Some(rest) = t.strip_prefix("```") else { return t };
    let rest = rest.split_once('\n').map(|(_, r)| r).unwrap_or(rest);
    rest.trim_end().strip_suffix("```").unwrap_or(rest).trim()
}

/// Sendet und weicht bei vorübergehenden Fehlern zurück.
async fn post(api_key: &str, body: &serde_json::Value) -> Result<String> {
    let mut body = body.clone();
    let mut attempt = 0u32;
    let mut stripped = false;
    let model = model_of(&body).to_string();

    loop {
        super::throttle::wait_turn(&model).await;

        let response = http::client()
            .post(CONVERSATIONS_URL)
            .bearer_auth(api_key)
            .json(&body)
            .timeout(Duration::from_secs(120))
            .send()
            .await?;

        let status = response.status();
        super::throttle::observe(&model, response.headers());

        if status == reqwest::StatusCode::UNAUTHORIZED {
            return Err(Error::other("Der Schlüssel wurde abgelehnt"));
        }

        // Eine 422 heisst meist nicht "Modell unbrauchbar", sondern
        // "diesen Zusatzparameter nehme ich nicht". Dann fliegt er raus
        // und es geht einmal erneut -- gemerkt, damit es künftig ohne
        // Fehlversuch klappt.
        if status == reqwest::StatusCode::UNPROCESSABLE_ENTITY && !stripped {
            if strip_completion_extras(&mut body) {
                super::capabilities::mark_rejected(&model, super::capabilities::Extra::Reasoning);
                stripped = true;
                attempt = 0;
                continue;
            }
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Ein Kontingent von null ist keine Überlastung, sondern eine
            // Sperre -- Zurückweichen hilft da nie.
            if let Some(0) = quota_of(response.headers()) {
                return Err(Error::ModelUnavailable {
                    model,
                    reason: "Dieser Schlüssel hat für diesen Endpunkt kein Kontingent \
                             (0 Anfragen je Minute). In der Mistral-Konsole unter \
                             „Limits“ nachsehen."
                        .into(),
                });
            }
        }

        if let Some(reason) = model_problem(status) {
            return Err(Error::ModelUnavailable { model, reason: reason.into() });
        }

        let retryable = status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status.is_server_error();

        if retryable && attempt < MAX_RETRIES {
            let delay = super::throttle::retry_after(response.headers()).unwrap_or_else(|| {
                Duration::from_millis((BASE_DELAY_MS * 2u64.pow(attempt)).min(MAX_DELAY_MS))
            });
            eprintln!(
                "[mistral] {status}, neuer Versuch in {:.1}s ({}/{MAX_RETRIES})",
                delay.as_secs_f64(),
                attempt + 1
            );
            tokio::time::sleep(delay).await;
            attempt += 1;
            continue;
        }

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(Error::ModelUnavailable {
                model,
                reason: "Die Anfragegrenze ist auch nach mehreren Versuchen erschöpft".into(),
            });
        }

        let parsed: ConversationResponse = response.error_for_status()?.json().await?;
        let text = extract_text(&parsed);

        if text.trim().is_empty() {
            let ans_budget = body["completion_args"]["max_tokens"].as_u64();
            let ausgeschoepft = budget_beim_nachdenken_verbraucht(parsed.usage.as_ref(), ans_budget);

            if ausgeschoepft && attempt < MAX_RETRIES {
                let hatte_reasoning = strip_completion_extras(&mut body);
                if hatte_reasoning {
                    // Das Nachdenken allein hat das Budget aufgebraucht -- der
                    // Text kam nie dran. Ohne Denkschritt einmal erneut
                    // versuchen, statt das Modell faelschlich als
                    // reasoning-unfaehig zu vermerken: es kann das durchaus,
                    // nur nicht bei diesem Umfang.
                    eprintln!(
                        "[mistral] Antwort leer, Budget beim Nachdenken erschöpft \
                         ({} Tokens) -- erneuter Versuch ohne reasoning_effort",
                        ans_budget.unwrap_or_default()
                    );
                    attempt += 1;
                    continue;
                }

                // Selbst ohne Denkschritt noch leer -- zweite Stufe: auch
                // die Werkzeuge fallen lassen. Ihre Treffer zaehlen zum
                // selben Budget und koennen es bei einer grossen Anfrage
                // allein ausschoepfen.
                if strip_tools(&mut body) {
                    eprintln!(
                        "[mistral] Antwort weiterhin leer -- erneuter Versuch \
                         ohne Werkzeuge ({} Tokens)",
                        ans_budget.unwrap_or_default()
                    );
                    attempt += 1;
                    continue;
                }
            }

            return Err(Error::other(
                "Das Modell lieferte keine Antwort -- vermutlich hat das \
                 Nachdenken oder die Werkzeuge das gesamte Tokenbudget \
                 aufgebraucht",
            ));
        }
        return Ok(text);
    }
}

/// Hat das Nachdenken allein das Antwortbudget aufgebraucht, bevor
/// ueberhaupt Text entstehen konnte?
///
/// Bei `reasoning_effort: high` zaehlt der Gedankengang mit zum
/// Antwortkontingent. Bei einem grossen Auftrag -- etwa dem Zuschneiden
/// des ganzen Fragenkatalogs -- kann er das Kontingent allein ausschoepfen;
/// die eigentliche Antwort kommt dann nie dran. Das ist kein Zeichen, dass
/// das Modell reasoning_effort generell nicht koennte, nur dass hier kein
/// Platz mehr war -- ein Fall fuer einen Versuch ohne Denkschritt, nicht
/// fuer eine dauerhafte Sperre.
fn budget_beim_nachdenken_verbraucht(usage: Option<&Usage>, max_tokens: Option<u64>) -> bool {
    match (usage, max_tokens) {
        (Some(u), Some(budget)) => u.completion_tokens >= budget,
        _ => false,
    }
}

/// Entfernt `reasoning_effort` aus `completion_args`.
/// Gibt zurück, ob überhaupt etwas zu entfernen war.
fn strip_completion_extras(body: &mut serde_json::Value) -> bool {
    body.get_mut("completion_args")
        .and_then(|a| a.as_object_mut())
        .map(|a| a.remove("reasoning_effort").is_some())
        .unwrap_or(false)
}

/// Leert `tools`, sofern dort noch welche standen. Gibt zurueck, ob
/// ueberhaupt etwas zu entfernen war -- sonst wuerde ein zweiter Versuch
/// nichts aendern und nur denselben Fehlschlag wiederholen.
fn strip_tools(body: &mut serde_json::Value) -> bool {
    let hatte_welche = body.get("tools")
        .and_then(|t| t.as_array())
        .is_some_and(|a| !a.is_empty());
    if hatte_welche {
        body["tools"] = serde_json::Value::Array(Vec::new());
    }
    hatte_welche
}

/* ---- Live-Ereignisse ----------------------------------------------------
 *
 * Fuer den Assistenten: statt auf die fertige Antwort zu warten, wird
 * jeder Zwischenschritt sofort weitergereicht -- Denkschritt, Websuche,
 * Rueckfaelle. Live gegen den echten Mistral-Schluessel geprueft, nicht
 * angenommen: `/v1/conversations` mit "stream": true liefert echtes SSE,
 * mit eigenen Ereignistypen fuer Denken, Werkzeugausfuehrung und Ende.
 * ------------------------------------------------------------------------- */

#[derive(Debug, Clone, PartialEq)]
pub enum AiEvent {
    /// Ein Stueck Gedankengang. Wird in Haeppchen gepuffert weitergereicht
    /// (siehe `stream_once`), nicht Zeichen fuer Zeichen -- sonst waere es
    /// mehr Ereignisse, als die Oberflaeche sinnvoll zeigen kann.
    ThinkingDelta(String),
    /// Die eigentliche Antwort faengt an zu entstehen. Sie ist JSON und
    /// wird bewusst nicht zeichenweise gezeigt -- waehrend sie waechst,
    /// saehe sie nur nach kaputtem Text aus.
    WritingAnswer,
    ToolStarted { name: String },
    /// Rohes Stueck der Werkzeug-Eingabe (meist eine wachsende
    /// JSON-Zeichenkette) -- fuer eine Live-Anzeige "sucht nach: …".
    ToolArgsDelta { delta: String },
    ToolDone { query: Option<String>, results: Vec<SearchHit> },
    Retry { reason: String },
    Done { prompt_tokens: u64, completion_tokens: u64 },
}

/// Ein kuratierter Websuche-Treffer -- Titel, Domain fuer die Anzeige,
/// volle Adresse zum Oeffnen.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchHit {
    pub title: String,
    pub domain: String,
    pub url: String,
}

/// Empfaengt Live-Ereignisse eines Modellaufrufs. In der Anwendung ein
/// Tauri-Ereignis (siehe `bridge.rs`), in Aufrufen ohne Oberflaeche ein
/// stiller Leerempfaenger.
pub trait AiEvents: Send + Sync {
    fn emit(&self, step: &str, event: AiEvent);
}

pub struct NoAiEvents;
impl AiEvents for NoAiEvents {
    fn emit(&self, _step: &str, _event: AiEvent) {}
}

/// Zerlegt einen einzelnen SSE-Block (durch eine Leerzeile vom naechsten
/// getrennt) in Ereignisname und Nutzlast.
fn parse_sse_block(block: &str) -> Option<(String, serde_json::Value)> {
    let mut event = None;
    let mut data = None;
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("event: ") {
            event = Some(rest.trim().to_string());
        } else if let Some(rest) = line.strip_prefix("data: ") {
            data = serde_json::from_str(rest.trim()).ok();
        }
    }
    Some((event?, data?))
}

/// Traegt zwischen zwei SSE-Bloecken die Teile einer Werkzeugausfuehrung
/// zusammen, die haeppchenweise ankommen (`started` bringt den Anfang,
/// jedes `delta` haengt an, `done` bringt erst die Treffer).
#[derive(Default)]
struct StreamState {
    tool_args: HashMap<String, String>,
}

impl StreamState {
    fn interpret(&mut self, event: &str, data: &serde_json::Value) -> Vec<AiEvent> {
        match event {
            "message.output.delta" => {
                let content = &data["content"];
                if let Some(s) = content.as_str() {
                    if s.is_empty() { vec![] } else { vec![AiEvent::WritingAnswer] }
                } else if content.get("type").and_then(|t| t.as_str()) == Some("thinking") {
                    let text: String = content["thinking"]
                        .as_array()
                        .map(|arr| arr.iter().filter_map(|b| b["text"].as_str()).collect::<String>())
                        .unwrap_or_default();
                    if text.is_empty() { vec![] } else { vec![AiEvent::ThinkingDelta(text)] }
                } else {
                    vec![]
                }
            }
            "tool.execution.started" | "tool.execution.delta" => {
                let id = data["id"].as_str().unwrap_or_default().to_string();
                let delta = data["arguments"].as_str().unwrap_or_default().to_string();
                self.tool_args.entry(id).or_default().push_str(&delta);

                let mut out = Vec::new();
                if event == "tool.execution.started" {
                    let name = data["name"].as_str().unwrap_or("Werkzeug").to_string();
                    out.push(AiEvent::ToolStarted { name });
                }
                if !delta.is_empty() {
                    out.push(AiEvent::ToolArgsDelta { delta });
                }
                out
            }
            "tool.execution.done" => {
                let id = data["id"].as_str().unwrap_or_default();
                let query = self
                    .tool_args
                    .remove(id)
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
                    .and_then(|v| v.get("query").and_then(|q| q.as_str()).map(str::to_string));
                let results = data["info"]["result"]
                    .as_str()
                    .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                    .map(|v| kuratierte_treffer(&v))
                    .unwrap_or_default();
                vec![AiEvent::ToolDone { query, results }]
            }
            "conversation.response.done" => {
                vec![AiEvent::Done {
                    prompt_tokens: data["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                    completion_tokens: data["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                }]
            }
            _ => vec![],
        }
    }
}

/// Reduziert die vollen Websuche-Treffer auf das, was eine Oberflaeche
/// zeigen kann: Titel und Domain, nicht mehrere Kilobyte Rohtext.
fn kuratierte_treffer(v: &serde_json::Value) -> Vec<SearchHit> {
    let Some(obj) = v.as_object() else { return Vec::new() };

    let mut mit_rang: Vec<(i64, SearchHit)> = obj
        .values()
        .filter_map(|eintrag| {
            let title = eintrag.get("title")?.as_str()?.to_string();
            let url = eintrag.get("url")?.as_str()?.to_string();
            let domain = url.split("://").nth(1)?.split('/').next()?.to_string();
            let rank = eintrag.get("rank").and_then(|r| r.as_i64()).unwrap_or(999);
            Some((rank, SearchHit { title, domain, url }))
        })
        .collect();

    mit_rang.sort_by_key(|(rang, _)| *rang);
    mit_rang.into_iter().map(|(_, hit)| hit).take(5).collect()
}

/// Wie `ask_json_with`, aber mit Zwischenstaenden waehrend die Antwort
/// entsteht. Traegt dieselbe zweistufige Absicherung gegen ein leeres
/// Ergebnis wie `post()`: erst ohne Denkschritt, dann ohne Werkzeuge.
pub async fn ask_json_streaming(
    api_key: &str,
    model: &str,
    system: &str,
    user: &str,
    tools: &[Tool],
    step: &str,
    events: &dyn AiEvents,
) -> Result<String> {
    let mut body = build_body(model, system, user, reasoning_wanted(), tools);
    let mut versuch = 0u32;

    loop {
        let text = stream_once(api_key, &body, step, events).await?;
        if !text.trim().is_empty() {
            return Ok(strip_fence(&text).to_string());
        }

        if versuch >= 2 {
            return Err(Error::other(
                "Das Modell lieferte keine Antwort -- vermutlich hat das                  Nachdenken oder die Werkzeuge das gesamte Tokenbudget aufgebraucht",
            ));
        }
        if strip_completion_extras(&mut body) {
            events.emit(step, AiEvent::Retry {
                reason: "Budget beim Nachdenken erschöpft — erneuter Versuch ohne Denkschritt"
                    .into(),
            });
        } else if strip_tools(&mut body) {
            events.emit(step, AiEvent::Retry {
                reason: "Antwort weiterhin leer — erneuter Versuch ohne Werkzeuge".into(),
            });
        } else {
            return Err(Error::other("Das Modell lieferte keine Antwort"));
        }
        versuch += 1;
    }
}

/// Wie viele Zeichen Denktext gesammelt werden, bevor ein Ereignis
/// hinausgeht. Ohne das waere es ein Tauri-Ereignis pro Wortfragment --
/// im echten Test alle 15 bis 20 Millisekunden eines.
const THINKING_BATCH_CHARS: usize = 60;

async fn stream_once(
    api_key: &str,
    body: &serde_json::Value,
    step: &str,
    events: &dyn AiEvents,
) -> Result<String> {
    let mut body = body.clone();
    body["stream"] = serde_json::json!(true);
    let model = model_of(&body).to_string();

    super::throttle::wait_turn(&model).await;
    let response = http::client()
        .post(CONVERSATIONS_URL)
        .bearer_auth(api_key)
        .json(&body)
        .timeout(Duration::from_secs(120))
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::other("Der Schlüssel wurde abgelehnt"));
    }
    if !response.status().is_success() {
        return Err(Error::other(format!(
            "Die Anfrage schlug fehl (Status {})",
            response.status()
        )));
    }

    let mut byte_stream = response.bytes_stream();
    let mut rest = String::new();
    let mut text = String::new();
    let mut thinking_buf = String::new();
    let mut zustand = StreamState::default();
    let mut antwort_begonnen = false;

    while let Some(chunk) = byte_stream.next().await {
        rest.push_str(&String::from_utf8_lossy(&chunk?));

        while let Some(ende) = rest.find("

") {
            let block = rest[..ende].to_string();
            let uebrig = rest.split_off(ende + 2);
            rest = uebrig;

            let Some((event, data)) = parse_sse_block(&block) else { continue };
            for ev in zustand.interpret(&event, &data) {
                match ev {
                    AiEvent::WritingAnswer => {
                        if !antwort_begonnen {
                            antwort_begonnen = true;
                            events.emit(step, AiEvent::WritingAnswer);
                        }
                        if let Some(s) = data["content"].as_str() {
                            text.push_str(s);
                        }
                    }
                    AiEvent::ThinkingDelta(d) => {
                        thinking_buf.push_str(&d);
                        if thinking_buf.len() >= THINKING_BATCH_CHARS {
                            events.emit(
                                step,
                                AiEvent::ThinkingDelta(std::mem::take(&mut thinking_buf)),
                            );
                        }
                    }
                    other => events.emit(step, other),
                }
            }
        }
    }

    if !thinking_buf.is_empty() {
        events.emit(step, AiEvent::ThinkingDelta(thinking_buf));
    }
    Ok(text)
}

/// Liest das verbleibende Kontingent aus den Antwortköpfen.
fn quota_of(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    super::throttle::read_headers(headers).limit
}

/// Wuerde ein anderes Modell helfen?
fn model_problem(status: reqwest::StatusCode) -> Option<&'static str> {
    match status.as_u16() {
        404 => Some("Kennung unbekannt oder zurückgezogen"),
        403 => Some("Der Tarif dieses Schlüssels schaltet das Modell nicht frei"),
        _ => None,
    }
}

fn model_of(body: &serde_json::Value) -> &str {
    body.get("model").and_then(|m| m.as_str()).unwrap_or("unbekannt")
}

/* ---- Absturzanalyse ----------------------------------------------------- */

#[derive(Debug, Deserialize)]
struct ModelVerdict {
    cause: String,
    explanation: String,
    #[serde(default)]
    suggestions: Vec<String>,
    #[serde(default)]
    culprit_mod: Option<String>,
    #[serde(default)]
    confidence: Option<u8>,
}

const SYSTEM_PROMPT: &str = "\
Du bist ein Fachmann für Minecraft-Startprobleme. Du bekommst das Ende \
eines Absturzprotokolls und sollst die auslösende Ursache benennen.

Antworte ausschließlich mit einem JSON-Objekt, ohne Text davor oder danach, \
ohne Code-Zaun. Felder:
  cause        Kurze Benennung der Ursache, höchstens acht Wörter, Deutsch.
  explanation  Zwei bis vier Sätze, warum es dazu kam. Deutsch, sachlich.
  suggestions  Zwei bis vier konkrete Handlungsschritte, Deutsch.
  culprit_mod  Kennung der verantwortlichen Mod, oder null.
  confidence   Ganzzahl 0-100, wie sicher du bist.

Nenne nur, was im Protokoll belegt ist. Rate keine Mod-Namen. Ist die \
Ursache nicht erkennbar, setze confidence niedrig und sage das in \
explanation.

Stehen dir Werkzeuge zur Verfügung, nutze sie: die Codeausführung, um die \
Stapelspur zu zerlegen und Versionsbereiche zu prüfen, die Websuche, um eine \
unbekannte Fehlersignatur nachzuschlagen. Übernimm aus Suchtreffern nur, was \
zum vorliegenden Protokoll passt.";

/// Wertet ein Protokoll durch das Modell aus.
pub async fn analyze(
    api_key: &str,
    model: &str,
    log: &str,
    fallback: Option<&Diagnosis>,
) -> Result<Diagnosis> {
    let tail: Vec<&str> = log.lines().rev().take(TAIL_LINES).collect();
    let tail: String = tail.into_iter().rev().collect::<Vec<_>>().join("\n");

    let mut user = String::new();
    if let Some(f) = fallback {
        user.push_str(&format!(
            "Eine regelbasierte Vorprüfung vermutet: \"{}\" (Sicherheit {}%). \
             Prüfe das und widersprich, wenn es nicht stimmt.\n\n",
            f.cause, f.confidence
        ));
    }
    user.push_str("Protokollende:\n\n");
    user.push_str(&tail);

    // Nachschlagen nur, wenn die Regeltafel nichts Belastbares hatte.
    // Bei einem erkannten Fehlerbild steht die Antwort fest, und die Suche
    // kostete nur Zeit und Kontingent.
    let unsicher = fallback.map(|f| f.confidence < 40).unwrap_or(true);
    let erlaubt = crate::store::load_config().ai_tools;
    let tools: &[Tool] = if unsicher && erlaubt {
        // Nachschlagen und Zerlegen ergänzen einander: das eine findet
        // fremde Berichte, das andere bringt Ordnung in die Stapelspur.
        &[Tool::WebSearch, Tool::CodeInterpreter]
    } else {
        &[]
    };

    let raw = ask_json_with(api_key, model, SYSTEM_PROMPT, &user, tools).await?;
    let verdict: ModelVerdict = serde_json::from_str(&raw)
        .map_err(|e| Error::other(format!("Antwort des Modells unlesbar: {e}")))?;

    Ok(Diagnosis {
        cause: verdict.cause,
        explanation: verdict.explanation,
        suggestions: verdict.suggestions,
        culprit_mod: verdict.culprit_mod,
        trigger_line: fallback.map(|f| f.trigger_line.clone()).unwrap_or_default(),
        confidence: verdict.confidence.unwrap_or(60).min(100),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Echte SSE-Bloecke, so wie Mistral sie tatsaechlich schickt --
    /// live gegen den Schluessel erfasst, nicht aus der Dokumentation
    /// abgetippt.
    #[test]
    fn denkschritt_wird_zeichenweise_erkannt() {
        let block = "event: message.output.delta\n\
            data: {\"type\":\"message.output.delta\",\"content\":{\"type\":\"thinking\",\
            \"thinking\":[{\"type\":\"text\",\"text\":\" Ben\"}],\"closed\":true}}";
        let (event, data) = parse_sse_block(block).unwrap();
        let mut zustand = StreamState::default();
        let ev = zustand.interpret(&event, &data);
        assert_eq!(ev, vec![AiEvent::ThinkingDelta(" Ben".into())]);
    }

    #[test]
    fn text_nach_dem_denken_wird_als_antwortbeginn_erkannt() {
        // Der Uebergang: content wird von einem Objekt zu einer blossen
        // Zeichenkette -- genau daran wird "jetzt kommt die Antwort" erkannt.
        let block = "event: message.output.delta\n\
            data: {\"type\":\"message.output.delta\",\"content\":\"uckler und L\"}";
        let (event, data) = parse_sse_block(block).unwrap();
        let mut zustand = StreamState::default();
        assert_eq!(zustand.interpret(&event, &data), vec![AiEvent::WritingAnswer]);
    }

    #[test]
    fn werkzeugstart_liefert_namen_und_ersten_bruchteil() {
        let block = "event: tool.execution.started\n\
            data: {\"id\":\"tool_1\",\"name\":\"web_search\",\
            \"arguments\":\"{\\\"query\",\"function\":\"web_search\"}";
        let (event, data) = parse_sse_block(block).unwrap();
        let mut zustand = StreamState::default();
        let ev = zustand.interpret(&event, &data);
        assert_eq!(ev, vec![
            AiEvent::ToolStarted { name: "web_search".into() },
            AiEvent::ToolArgsDelta { delta: "{\"query".into() },
        ]);
    }

    #[test]
    fn werkzeugfragmente_werden_ueber_die_kennung_zusammengefuehrt() {
        let mut zustand = StreamState::default();
        let start = parse_sse_block(
            "event: tool.execution.started\n\
             data: {\"id\":\"t1\",\"name\":\"web_search\",\"arguments\":\"{\\\"query\\\": \\\"sod\"}",
        ).unwrap();
        zustand.interpret(&start.0, &start.1);
        let delta = parse_sse_block(
            "event: tool.execution.delta\n\
             data: {\"id\":\"t1\",\"name\":\"web_search\",\"arguments\":\"ium\\\"}\"}",
        ).unwrap();
        zustand.interpret(&delta.0, &delta.1);

        assert_eq!(zustand.tool_args.get("t1").unwrap(), "{\"query\": \"sodium\"}");
    }

    #[test]
    fn werkzeugende_liest_treffer_und_ursprungsfrage() {
        let mut zustand = StreamState::default();
        zustand.tool_args.insert("t1".into(), r#"{"query": "sodium fps"}"#.into());

        // "result" traegt selbst wieder JSON, als Zeichenkette verpackt --
        // deshalb die verschachtelten Anfuehrungszeichen mit \" statt eines
        // rohen Strings, damit sie tatsaechlich escaped im Text landen.
        let block = "event: tool.execution.done\ndata: {\"id\":\"t1\",\"name\":\"web_search\",\"info\":{\"result\":\"{\\\"a\\\":{\\\"title\\\":\\\"Sodium Mod\\\",\\\"url\\\":\\\"https://modrinth.com/mod/sodium\\\",\\\"rank\\\":0},\\\"b\\\":{\\\"title\\\":\\\"Zweiter Treffer\\\",\\\"url\\\":\\\"https://reddit.com/r/x\\\",\\\"rank\\\":1}}\"}}";
        let (event, data) = parse_sse_block(block).unwrap();
        let ev = zustand.interpret(&event, &data);

        assert_eq!(ev.len(), 1);
        let AiEvent::ToolDone { query, results } = &ev[0] else { panic!("falscher Typ: {ev:?}") };
        assert_eq!(query.as_deref(), Some("sodium fps"));
        assert_eq!(results, &vec![
            SearchHit {
                title: "Sodium Mod".into(), domain: "modrinth.com".into(),
                url: "https://modrinth.com/mod/sodium".into(),
            },
            SearchHit {
                title: "Zweiter Treffer".into(), domain: "reddit.com".into(),
                url: "https://reddit.com/r/x".into(),
            },
        ]);
    }

    #[test]
    fn ende_liest_den_verbrauch() {
        let block = r#"event: conversation.response.done
data: {"type":"conversation.response.done","usage":{"prompt_tokens":24,"completion_tokens":172,"total_tokens":196}}"#;
        let (event, data) = parse_sse_block(block).unwrap();
        let mut zustand = StreamState::default();
        assert_eq!(
            zustand.interpret(&event, &data),
            vec![AiEvent::Done { prompt_tokens: 24, completion_tokens: 172 }],
        );
    }

    #[test]
    fn unbekannte_ereignisse_werden_ignoriert() {
        let block = "event: conversation.response.started\n\
            data: {\"type\":\"conversation.response.started\",\"conversation_id\":\"x\"}";
        let (event, data) = parse_sse_block(block).unwrap();
        let mut zustand = StreamState::default();
        assert!(zustand.interpret(&event, &data).is_empty());
    }

    #[test]
    fn code_zaun_wird_entfernt() {
        assert_eq!(strip_fence("{\"a\":1}"), "{\"a\":1}");
        assert_eq!(strip_fence("```json\n{\"a\":1}\n```"), "{\"a\":1}");
        assert_eq!(strip_fence("```\n{\"a\":1}\n```"), "{\"a\":1}");
    }

    #[test]
    fn reasoning_steht_in_completion_args() {
        // Auf der obersten Ebene wird er abgelehnt -- das war der Fehler.
        let body = build_body("mistral-medium-latest", "auftrag", "frage", true, &[]);
        assert_eq!(body["completion_args"]["reasoning_effort"], "high");
        assert!(body.get("reasoning_effort").is_none());
    }

    #[test]
    fn koerper_nutzt_die_felder_des_conversations_endpunkts() {
        let body = build_body("m", "auftrag", "frage", false, &[]);
        assert_eq!(body["instructions"], "auftrag");
        assert_eq!(body["inputs"][0]["content"], "frage");
        assert_eq!(body["inputs"][0]["role"], "user");
        // Kein `messages`, kein `response_format`.
        assert!(body.get("messages").is_none());
        assert!(body.get("response_format").is_none());
    }

    #[test]
    fn gedankengang_wird_uebersprungen() {
        // So kam die echte Antwort zurück: erst ein thinking-Block, dann der Text.
        let raw = r#"{"outputs":[{"content":[
            {"type":"thinking","thinking":[{"type":"text","text":"Ich überlege..."}]},
            {"type":"text","text":"Die Antwort."}
        ]}]}"#;
        let parsed: ConversationResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(extract_text(&parsed), "Die Antwort.");
    }

    #[test]
    fn inhalt_als_blosse_zeichenkette() {
        let raw = r#"{"outputs":[{"content":"direkt"}]}"#;
        let parsed: ConversationResponse = serde_json::from_str(raw).unwrap();
        assert_eq!(extract_text(&parsed), "direkt");
    }

    #[test]
    fn leere_antwort_ergibt_leeren_text() {
        let parsed: ConversationResponse = serde_json::from_str(r#"{"outputs":[]}"#).unwrap();
        assert!(extract_text(&parsed).is_empty());
    }

    /// Das echte Ereignis: 5373 Eingabe- plus 2048 Ausgabe-Tokens, davon
    /// ging jedes einzelne ins Nachdenken -- kein Text blieb uebrig.
    #[test]
    fn erschoepftes_budget_beim_nachdenken_wird_erkannt() {
        let raw = r#"{"outputs":[{"content":[
            {"type":"thinking","thinking":[{"type":"text","text":"..."}]}
        ]}],"usage":{"completion_tokens":2048}}"#;
        let parsed: ConversationResponse = serde_json::from_str(raw).unwrap();
        assert!(extract_text(&parsed).is_empty(), "kein Text-Block vorhanden");
        assert!(budget_beim_nachdenken_verbraucht(parsed.usage.as_ref(), Some(2048)));
    }

    #[test]
    fn knapp_verfehltes_budget_zaehlt_nicht_als_erschoepft() {
        let usage = Usage { completion_tokens: 1200 };
        assert!(!budget_beim_nachdenken_verbraucht(Some(&usage), Some(2048)));
    }

    /// Zweite Rueckfallstufe: bleibt die Antwort auch ohne Denkschritt
    /// leer, faellt zuletzt auch die Websuche weg -- ihre Treffer zaehlen
    /// zum selben Budget.
    #[test]
    fn werkzeuge_lassen_sich_bei_erschoepftem_budget_abwerfen() {
        let mut body = build_body("m", "a", "b", false, &[Tool::WebSearch]);
        assert!(body["tools"].as_array().is_some_and(|a| !a.is_empty()));

        assert!(strip_tools(&mut body));
        assert!(body["tools"].as_array().unwrap().is_empty());

        // Ein zweites Mal gibt es nichts mehr zu entfernen.
        assert!(!strip_tools(&mut body));
    }

    #[test]
    fn ohne_werkzeuge_gibt_es_bei_der_ruecknahme_nichts_zu_tun() {
        let mut body = build_body("m", "a", "b", false, &[]);
        assert!(!strip_tools(&mut body));
    }

    #[test]
    fn werkzeuge_heben_das_grundbudget_an() {
        // Websuche haengt ihre Treffer an die Antwort -- mehrere Kilobyte
        // im echten Test. Beim Grundbudget waere dafuer kaum Platz.
        let ohne = build_body("m", "a", "b", false, &[]);
        let mit = build_body("m", "a", "b", false, &[Tool::WebSearch]);
        assert_eq!(ohne["completion_args"]["max_tokens"], 4096);
        assert_eq!(mit["completion_args"]["max_tokens"], 8192);
    }

    #[test]
    fn ohne_nutzungsangabe_wird_nichts_vermutet() {
        // Kein `usage`-Feld in der Antwort -- dann lieber ehrlich "leer"
        // melden als grundlos einen zweiten Versuch zu starten.
        assert!(!budget_beim_nachdenken_verbraucht(None, Some(2048)));
    }

    #[test]
    fn zusatzparameter_laesst_sich_abwerfen() {
        let mut body = build_body("m", "a", "b", true, &[]);
        assert!(strip_completion_extras(&mut body));
        assert!(body["completion_args"].get("reasoning_effort").is_none());
        // Ein zweites Mal gibt es nichts mehr zu entfernen.
        assert!(!strip_completion_extras(&mut body));
        // Der Rest bleibt stehen.
        assert_eq!(body["completion_args"]["temperature"], 0.2);
    }

    /// Beide Suchwerkzeuge zusammen lehnt die Schnittstelle ab.
    #[test]
    fn suchwerkzeuge_schliessen_einander_aus() {
        let beide = sanitize_tools(vec![Tool::WebSearch, Tool::WebSearchPremium]);
        assert_eq!(beide, vec![Tool::WebSearchPremium], "die gründlichere gewinnt");

        // Einzeln bleibt jedes erhalten.
        assert_eq!(sanitize_tools(vec![Tool::WebSearch]), vec![Tool::WebSearch]);
        assert!(sanitize_tools(vec![]).is_empty());
    }

    #[test]
    fn suche_und_codeausfuehrung_vertragen_sich() {
        // Nur die beiden Suchvarianten schließen einander aus.
        let beide = sanitize_tools(vec![Tool::WebSearch, Tool::CodeInterpreter]);
        assert_eq!(beide.len(), 2);

        let mit_premium =
            sanitize_tools(vec![Tool::WebSearch, Tool::WebSearchPremium, Tool::CodeInterpreter]);
        assert_eq!(mit_premium, vec![Tool::WebSearchPremium, Tool::CodeInterpreter]);
    }

    #[test]
    fn werkzeuge_landen_im_koerper() {
        let body = build_body("m", "a", "b", false, &[Tool::WebSearch, Tool::CodeInterpreter]);
        assert_eq!(body["tools"][0]["type"], "web_search");
        assert_eq!(body["tools"][1]["type"], "code_interpreter");

        // Ohne Werkzeuge gar kein Feld -- ein leeres wäre unnötiger Ballast.
        let ohne = build_body("m", "a", "b", false, &[]);
        assert!(ohne.get("tools").is_none());
    }

    #[test]
    fn statuscodes_werden_zugeordnet() {
        use reqwest::StatusCode;
        assert!(model_problem(StatusCode::NOT_FOUND).is_some());
        assert!(model_problem(StatusCode::FORBIDDEN).is_some());
        // Diese behebt ein anderes Modell nicht.
        assert!(model_problem(StatusCode::UNAUTHORIZED).is_none());
        assert!(model_problem(StatusCode::OK).is_none());
    }

    #[test]
    fn nur_brauchbare_modelle_stehen_zur_wahl() {
        // Die beiden Reihen, die chatten koennen.
        for id in [
            "mistral-medium-latest", "mistral-medium-3-5", "mistral-small-latest",
            "ministral-14b-latest", "ministral-3b-latest",
        ] {
            assert!(usable_model(id), "{id} fehlt in der Auswahl");
        }

        // Andere Reihen -- andere Zwecke.
        for id in ["codestral-latest", "pixtral-12b", "devstral-2512", "voxtral-mini"] {
            assert!(!usable_model(id), "{id} gehört nicht in die Auswahl");
        }

        // Richtiges Praefix, aber kein Chatmodell -- genau die loesen sonst
        // Fehlermeldungen aus, die niemand einordnen kann.
        for id in [
            "mistral-embed", "mistral-ocr-latest", "mistral-moderation-latest",
            "mistral-saba-transcribe",
        ] {
            assert!(!usable_model(id), "{id} kann nicht chatten");
        }
    }

    #[test]
    fn grossschreibung_stoert_nicht() {
        assert!(usable_model("Mistral-Medium-Latest"));
        assert!(!usable_model("Mistral-Embed"));
    }

    #[test]
    fn voreinstellungen_stehen_selbst_zur_wahl() {
        assert!(usable_model(DEFAULT_MODEL));
        assert!(usable_model(FALLBACK_MODEL));
    }

    #[test]
    fn voreinstellungen_nutzen_die_latest_form() {
        assert!(DEFAULT_MODEL.ends_with("-latest"));
        assert!(FALLBACK_MODEL.ends_with("-latest"));
    }
}
