//! Ein Fehlertyp fuer das gesamte Backend.
//!
//! Tauri-Commands koennen nur Fehler zurueckgeben, die sich serialisieren
//! lassen. Deshalb wird `Error` als Zeichenkette uebertragen -- die Variante
//! bleibt im Log erhalten, der Oberflaeche reicht die Meldung.

use serde::{Serialize, Serializer};

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Datei-Zugriff fehlgeschlagen: {0}")]
    Io(#[from] std::io::Error),

    #[error("Netzwerkfehler: {0}")]
    Http(#[from] reqwest::Error),

    #[error("Ungültige Daten: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Archiv beschädigt: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("Prüfsumme stimmt nicht: {datei} (erwartet {erwartet}, erhalten {erhalten})")]
    Checksum {
        datei: String,
        erwartet: String,
        erhalten: String,
    },

    #[error("Nicht gefunden: {0}")]
    NotFound(String),

    #[error("Kein passendes Java gefunden (benötigt wird Version {0} oder neuer)")]
    NoJava(u32),

    #[error("Die Instanz läuft bereits")]
    AlreadyRunning,

    #[error("Kein Konto ausgewählt")]
    NoAccount,

    /// Das gewuenschte Modell steht nicht zur Verfuegung.
    ///
    /// Eigene Variante, weil sie anders behandelt wird als ein gewoehnlicher
    /// Fehler: die Oberflaeche fragt daraufhin, ob mit einem Ersatzmodell
    /// weitergemacht werden soll. Automatisch herabzustufen waere falsch --
    /// ein schwaecheres Modell liefert schlechtere Vorschlaege, und das
    /// muss sichtbar sein, nicht stillschweigend geschehen.
    #[error("Modell {model} ist nicht verfügbar: {reason}")]
    ModelUnavailable { model: String, reason: String },

    #[error("{0}")]
    Other(String),
}

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Error::NotFound(msg.into())
    }

    /// Ist das ein Fehler, bei dem ein Ersatzmodell helfen wuerde?
    pub fn is_model_unavailable(&self) -> bool {
        matches!(self, Error::ModelUnavailable { .. })
    }
}

impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
