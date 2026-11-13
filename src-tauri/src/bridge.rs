//! Verbindung zwischen Backend und Oberflaeche.
//!
//! Fortschritt und Ereignisse gehen als Tauri-Ereignisse hinaus. Die
//! Fachmodule kennen Tauri nicht -- sie bekommen diese Umsetzungen
//! hereingereicht und bleiben dadurch ohne laufende Oberflaeche pruefbar.

use std::sync::Arc;

use tauri::{AppHandle, Emitter};

use crate::ai::client::{AiEvent, AiEvents};
use crate::download::{Progress, Sink};
use crate::launch::process::Events;

pub struct TauriSink {
    app: AppHandle,
    /// Name des Ereignisses, damit Version und Modpack getrennt melden.
    event: &'static str,
}

impl TauriSink {
    pub fn new(app: AppHandle, event: &'static str) -> Arc<dyn Sink> {
        Arc::new(Self { app, event })
    }
}

impl Sink for TauriSink {
    fn emit(&self, p: &Progress) {
        let _ = self.app.emit(self.event, p);
    }
}

pub struct TauriEvents {
    app: AppHandle,
}

impl TauriEvents {
    pub fn new(app: AppHandle) -> Arc<dyn Events> {
        Arc::new(Self { app })
    }
}

impl Events for TauriEvents {
    fn log_line(&self, instance_id: &str, line: &str) {
        let _ = self.app.emit(
            "log-line",
            serde_json::json!({ "instance_id": instance_id, "line": line }),
        );
    }

    fn stopped(&self, instance_id: &str, exit_code: i32, played_seconds: u64) {
        crate::launch::record_playtime(instance_id, played_seconds);

        // Bei einem Fehlschlag gleich auswerten -- der Nutzer soll nicht
        // erst selbst auf die Idee kommen muessen, ins Protokoll zu sehen.
        if exit_code != 0 {
            if let Ok(Some(d)) = crate::launch::analyze_instance(instance_id) {
                let _ = self.app.emit(
                    "instance-crashed",
                    serde_json::json!({
                        "instance_id": instance_id,
                        "exit_code": exit_code,
                        "diagnosis": d,
                    }),
                );
            }
        }

        let _ = self.app.emit(
            "instance-stopped",
            serde_json::json!({
                "instance_id": instance_id,
                "exit_code": exit_code,
                "played_seconds": played_seconds,
            }),
        );
    }
}

/// Gegenstueck zu `TauriEvents`, aber fuer Bedrock -- ruft bewusst weder
/// `record_playtime` noch `analyze_instance` auf: `bedrock::launch::start`
/// schreibt die Spielzeit schon selbst zurueck, und Javas
/// Absturz-Heuristik passt nicht auf Bedrock-Protokolle.
pub struct TauriBedrockEvents {
    app: AppHandle,
}

impl TauriBedrockEvents {
    pub fn new(app: AppHandle) -> Arc<dyn crate::bedrock::launch::BedrockEvents> {
        Arc::new(Self { app })
    }
}

impl crate::bedrock::launch::BedrockEvents for TauriBedrockEvents {
    fn stopped(&self, instance_id: &str, exit_code: i32, played_seconds: u64) {
        let _ = self.app.emit(
            "bedrock-instance-stopped",
            serde_json::json!({
                "instance_id": instance_id,
                "exit_code": exit_code,
                "played_seconds": played_seconds,
            }),
        );
    }
}

/// Rust-Edition: Protokollzeilen und Spielende an die Oberflaeche.
pub struct TauriRustEvents {
    app: AppHandle,
}

impl TauriRustEvents {
    pub fn new(app: AppHandle) -> Arc<dyn crate::rust_edition::launch::RustEvents> {
        Arc::new(Self { app })
    }
}

impl crate::rust_edition::launch::RustEvents for TauriRustEvents {
    fn log_line(&self, instance_id: &str, line: &str) {
        let _ = self.app.emit("rust-log-line", serde_json::json!({ "instance_id": instance_id, "line": line }));
    }

    fn stopped(&self, instance_id: &str, exit_code: i32, played_seconds: u64, crash_report: Option<String>) {
        let _ = self.app.emit(
            "rust-instance-stopped",
            serde_json::json!({
                "instance_id": instance_id,
                "exit_code": exit_code,
                "played_seconds": played_seconds,
                "crash_report": crash_report,
            }),
        );
    }
}

/// Reicht Zwischenschritte eines Modellaufrufs live an die Oberflaeche
/// weiter -- Denkschritt, Websuche, Rueckfaelle. Eine Instanz gehoert zu
/// genau einem Sitzungslauf, deshalb steht die Kennung fest statt bei
/// jedem Aufruf mitgegeben zu werden.
pub struct TauriAiEvents {
    app: AppHandle,
    session_id: String,
}

impl TauriAiEvents {
    pub fn new(app: AppHandle, session_id: String) -> Self {
        Self { app, session_id }
    }
}

impl AiEvents for TauriAiEvents {
    fn emit(&self, step: &str, event: AiEvent) {
        let payload = match event {
            AiEvent::ThinkingDelta(text) => {
                serde_json::json!({ "kind": "thinking", "text": text })
            }
            AiEvent::WritingAnswer => serde_json::json!({ "kind": "writing" }),
            AiEvent::ToolStarted { name } => {
                serde_json::json!({ "kind": "tool_started", "name": name })
            }
            AiEvent::ToolArgsDelta { delta } => {
                serde_json::json!({ "kind": "tool_args", "delta": delta })
            }
            AiEvent::ToolDone { query, results } => serde_json::json!({
                "kind": "tool_done",
                "query": query,
                "results": results.into_iter()
                    .map(|h| serde_json::json!({ "title": h.title, "domain": h.domain, "url": h.url }))
                    .collect::<Vec<_>>(),
            }),
            AiEvent::Retry { reason } => serde_json::json!({ "kind": "retry", "reason": reason }),
            AiEvent::Done { prompt_tokens, completion_tokens } => serde_json::json!({
                "kind": "done",
                "prompt_tokens": prompt_tokens,
                "completion_tokens": completion_tokens,
            }),
        };

        let mut voll = payload;
        voll["session_id"] = serde_json::json!(self.session_id);
        voll["step"] = serde_json::json!(step);
        let _ = self.app.emit("assistant-ai-log", voll);
    }
}

/// Sendet die Live-Werte laufender Spiele.
///
/// Ohne diesen Takt bliebe die Leistungsanzeige leer: der Befehl zum
/// Abfragen war da, das Ereignis im Frontend abonniert -- aber nichts
/// verband beide.
///
/// Laeuft nichts, kostet der Durchlauf nichts: `sample_all` liefert dann
/// eine leere Liste und es wird nichts gesendet.
pub fn start_telemetry(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut takt = tokio::time::interval(std::time::Duration::from_millis(1500));
        loop {
            takt.tick().await;
            for werte in crate::launch::telemetry::sample_all() {
                let _ = app.emit("telemetry", &werte);
            }
        }
    });
}
