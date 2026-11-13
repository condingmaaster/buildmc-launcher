//! Gespeicherte Server.

pub mod lan;
pub mod ping;

use serde::{Deserialize, Serialize};

pub use lan::{discover, LanServer};
pub use ping::{ping, ServerStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedServer {
    pub id: String,
    pub name: String,
    pub address: String,
    pub favorite: bool,
}

impl Default for SavedServer {
    fn default() -> Self {
        Self { id: String::new(), name: String::new(), address: String::new(), favorite: false }
    }
}

/// Zerlegt `host:port`. Ohne Port gilt Minecrafts Standard 25565.
pub fn split_address(address: &str) -> (String, u16) {
    match address.rsplit_once(':') {
        Some((host, port)) => match port.parse() {
            Ok(p) => (host.to_string(), p),
            // Kein Port, sondern Teil einer IPv6-Adresse o. ae.
            Err(_) => (address.to_string(), 25565),
        },
        None => (address.to_string(), 25565),
    }
}

#[cfg(test)]
mod tests {
    use super::split_address;

    #[test]
    fn adresse_wird_zerlegt() {
        assert_eq!(split_address("play.buildmc.io"), ("play.buildmc.io".into(), 25565));
        assert_eq!(split_address("localhost:25566"), ("localhost".into(), 25566));
        assert_eq!(split_address("192.168.1.42:25565"), ("192.168.1.42".into(), 25565));
    }
}
