//! Ein gemeinsamer HTTP-Client.
//!
//! Ein einziger Client fuer den gesamten Prozess: so bleiben
//! Verbindungspool und TLS-Sitzungen erhalten, was bei einigen hundert
//! Bibliotheks-Downloads deutlich mehr ausmacht als die Parallelitaet.

use std::sync::OnceLock;
use std::time::Duration;

use crate::error::Result;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(concat!("BuildMCLauncher/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            // Ohne das haengt eine Anfrage unbegrenzt, wenn ein Server die
            // Verbindung annimmt und dann einfach nie fertig antwortet --
            // `connect_timeout` allein deckt nur den Verbindungsaufbau ab.
            // Grosszuegig bemessen, damit ein grosser Download (das JDK,
            // ~180 MB) auf einer langsamen Leitung nicht faelschlich
            // abbricht; die Wiederholung pro Datei (`download::fetch_one`)
            // faengt einen echten Abbruch danach ohnehin ab.
            .timeout(Duration::from_secs(300))
            .pool_max_idle_per_host(16)
            .build()
            .expect("HTTP-Client konnte nicht erstellt werden")
    })
}

/// Laedt eine JSON-Ressource und deserialisiert sie.
pub async fn get_json<T: serde::de::DeserializeOwned>(url: &str) -> Result<T> {
    let bytes = client().get(url).send().await?.error_for_status()?.bytes().await?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Laedt eine Ressource vollstaendig in den Speicher.
pub async fn get_bytes(url: &str) -> Result<Vec<u8>> {
    let bytes = client().get(url).send().await?.error_for_status()?.bytes().await?;
    Ok(bytes.to_vec())
}

/// Wie `get_json`, aber mit einem Kopf-Schluessel -- fuer Portale wie
/// CurseForge, die einen `x-api-key` statt eines Bearer-Tokens verlangen.
pub async fn get_json_with_key<T: serde::de::DeserializeOwned>(
    url: &str,
    header: &str,
    key: &str,
) -> Result<T> {
    let bytes = client()
        .get(url)
        .header(header, key)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(serde_json::from_slice(&bytes)?)
}
