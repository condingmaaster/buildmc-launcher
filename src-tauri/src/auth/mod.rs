//! Konten: Microsoft-Anmeldung, Offline-Konten und Profile.

pub mod expiry;
pub mod microsoft;
pub mod offline;
pub mod profile;
pub mod skins;

pub use profile::{fetch_own, fetch_public, MinecraftProfile, PublicProfile};
pub use expiry::ExpiryFormat;
pub use skins::{OwnedCape, SkinVariant};

use serde::{Deserialize, Serialize};

/// Ein Konto. Feldnamen unveraendert aus 0.1.3.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Account {
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub id: String,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub username: String,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub uuid: String,
    /// `microsoft` oder `offline`.
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub account_type: String,
    pub active: bool,

    /// Nur bei Microsoft-Konten belegt.
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub access_token: String,
    #[serde(deserialize_with = "crate::serde_util::null_default")]
    pub refresh_token: String,
    /// Unix-Sekunden. Gelesen wird jedes gaengige Format -- Version 0.1.3
    /// schrieb hier eine RFC-3339-Zeichenkette.
    #[serde(with = "expiry")]
    pub expires_at: i64,

    // --- neu in 0.2 ---
    pub skin_url: Option<String>,
    pub cape_url: Option<String>,
}

impl Default for Account {
    fn default() -> Self {
        Self {
            id: String::new(), username: String::new(), uuid: String::new(),
            account_type: "offline".into(), active: false,
            access_token: String::new(), refresh_token: String::new(),
            expires_at: 0, skin_url: None, cape_url: None,
        }
    }
}

impl Account {
    pub fn is_offline(&self) -> bool {
        self.account_type == "offline"
    }

    /// Ist das Zugriffstoken noch gueltig? Eine Minute Sicherheitsabstand,
    /// damit es nicht mitten im Start ablaeuft.
    pub fn token_valid(&self) -> bool {
        !self.is_offline()
            && !self.access_token.is_empty()
            && self.expires_at > chrono::Utc::now().timestamp() + 60
    }
}
