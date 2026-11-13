//! Minecraft-Profil: Name, UUID, Skin und Umhang.
//!
//! Zwei Wege, je nachdem wessen Profil gemeint ist:
//!
//! * Das **eigene** Konto kommt von `api.minecraftservices.com` und braucht
//!   das Minecraft-Zugriffstoken. Es liefert die Skin-Adresse unmittelbar.
//! * **Fremde** Spieler kommen ohne Token vom Session-Server. Dort steckt
//!   die Skin-Adresse in einem Base64-Feld, das erst ausgepackt werden muss.
//!
//! Gebraucht wird beides: das eigene Profil fuer die Kontoanzeige, fremde
//! fuer alles, was andere Spieler zeigt.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http;

const PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";
const SESSION_URL: &str = "https://sessionserver.mojang.com/session/minecraft/profile";

/* ---- Eigenes Profil --------------------------------------------------- */

#[derive(Debug, Clone, Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub skins: Vec<Texture>,
    #[serde(default)]
    pub capes: Vec<Texture>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Texture {
    pub id: String,
    /// `ACTIVE` oder `INACTIVE` -- ein Konto kann mehrere Skins halten,
    /// getragen wird nur der aktive.
    pub state: String,
    pub url: String,
    /// `CLASSIC` oder `SLIM`. Bestimmt die Armbreite, nicht den Kopf.
    #[serde(default)]
    pub variant: Option<String>,
}

impl MinecraftProfile {
    pub fn active_skin(&self) -> Option<&Texture> {
        self.skins
            .iter()
            .find(|s| s.state.eq_ignore_ascii_case("ACTIVE"))
            .or_else(|| self.skins.first())
    }

    pub fn active_cape(&self) -> Option<&Texture> {
        self.capes.iter().find(|c| c.state.eq_ignore_ascii_case("ACTIVE"))
    }

    /// UUID mit Bindestrichen. Mojang liefert sie ohne.
    pub fn dashed_id(&self) -> String {
        dash_uuid(&self.id)
    }
}

/// Holt das Profil des angemeldeten Kontos.
pub async fn fetch_own(access_token: &str) -> Result<MinecraftProfile> {
    let response = http::client()
        .get(PROFILE_URL)
        .bearer_auth(access_token)
        .send()
        .await?;

    if response.status() == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::other("Sitzung abgelaufen, bitte neu anmelden"));
    }
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(Error::other(
            "Dieses Konto besitzt kein Minecraft: Java Edition",
        ));
    }

    Ok(response.error_for_status()?.json().await?)
}

/* ---- Fremdes Profil --------------------------------------------------- */

#[derive(Debug, Clone, Deserialize)]
struct SessionProfile {
    name: String,
    id: String,
    #[serde(default)]
    properties: Vec<SessionProperty>,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionProperty {
    name: String,
    /// Base64 eines JSON-Objekts mit den Texturen.
    value: String,
}

#[derive(Debug, Clone, Deserialize)]
struct TexturePayload {
    #[serde(default)]
    textures: TextureSet,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TextureSet {
    #[serde(rename = "SKIN")]
    skin: Option<TextureRef>,
    #[serde(rename = "CAPE")]
    cape: Option<TextureRef>,
}

#[derive(Debug, Clone, Deserialize)]
struct TextureRef {
    url: String,
}

/// Was von einem fremden Profil gebraucht wird.
#[derive(Debug, Clone, Serialize)]
pub struct PublicProfile {
    pub name: String,
    pub uuid: String,
    pub skin_url: Option<String>,
    pub cape_url: Option<String>,
}

/// Holt Name und Skin eines beliebigen Spielers -- ohne Anmeldung.
pub async fn fetch_public(uuid: &str) -> Result<PublicProfile> {
    let compact = uuid.replace('-', "");
    let profile: SessionProfile =
        http::get_json(&format!("{SESSION_URL}/{compact}")).await?;

    let payload = profile
        .properties
        .iter()
        .find(|p| p.name == "textures")
        .map(|p| decode_textures(&p.value))
        .transpose()?
        .unwrap_or_default();

    Ok(PublicProfile {
        name: profile.name,
        uuid: dash_uuid(&profile.id),
        skin_url: payload.skin.map(|t| https(t.url)),
        cape_url: payload.cape.map(|t| https(t.url)),
    })
}

fn decode_textures(value: &str) -> Result<TextureSet> {
    let raw = base64_decode(value)
        .ok_or_else(|| Error::other("Texturfeld des Profils ist unlesbar"))?;
    let payload: TexturePayload = serde_json::from_slice(&raw)?;
    Ok(payload.textures)
}

/// Mojang liefert die Skin-Adresse mit `http://`. In der Webansicht wird
/// gemischter Inhalt blockiert, deshalb auf `https://` heben -- derselbe
/// Server beantwortet beides.
fn https(url: String) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url,
    }
}

/// `694aa7122e20499eab330ada95d10400` wird zu
/// `694aa712-2e20-499e-ab33-0ada95d10400`.
pub fn dash_uuid(compact: &str) -> String {
    let c = compact.replace('-', "");
    if c.len() != 32 {
        return compact.to_string();
    }
    format!(
        "{}-{}-{}-{}-{}",
        &c[0..8], &c[8..12], &c[12..16], &c[16..20], &c[20..32]
    )
}

/// Base64 ohne zusaetzliche Abhaengigkeit -- die Standardtabelle reicht.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut lookup = [255u8; 256];
    for (i, b) in TABLE.iter().enumerate() {
        lookup[*b as usize] = i as u8;
    }

    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buffer = 0u32;
    let mut bits = 0u32;

    for byte in input.bytes() {
        if byte == b'=' || byte.is_ascii_whitespace() {
            continue;
        }
        let value = lookup[byte as usize];
        if value == 255 {
            return None;
        }
        buffer = (buffer << 6) | value as u32;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }

    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_bekommt_bindestriche() {
        assert_eq!(
            dash_uuid("694aa7122e20499eab330ada95d10400"),
            "694aa712-2e20-499e-ab33-0ada95d10400"
        );
        // Bereits gesetzte Bindestriche bleiben erhalten.
        assert_eq!(
            dash_uuid("694aa712-2e20-499e-ab33-0ada95d10400"),
            "694aa712-2e20-499e-ab33-0ada95d10400"
        );
        // Unerwartete Laenge wird unveraendert durchgereicht.
        assert_eq!(dash_uuid("kurz"), "kurz");
    }

    #[test]
    fn base64_entschluesselt() {
        assert_eq!(base64_decode("aGFsbG8=").unwrap(), b"hallo");
        assert_eq!(base64_decode("eyJhIjoxfQ==").unwrap(), b"{\"a\":1}");
        assert!(base64_decode("nicht base64 #").is_none());
    }

    #[test]
    fn texturfeld_wird_ausgepackt() {
        // Base64 von {"textures":{"SKIN":{"url":"http://x/y"}}}
        let value = "eyJ0ZXh0dXJlcyI6eyJTS0lOIjp7InVybCI6Imh0dHA6Ly94L3kifX19";
        let set = decode_textures(value).unwrap();
        assert_eq!(set.skin.unwrap().url, "http://x/y");
        assert!(set.cape.is_none());
    }

    #[test]
    fn skin_adresse_wird_auf_https_gehoben() {
        assert_eq!(https("http://textures.minecraft.net/a".into()),
                   "https://textures.minecraft.net/a");
        assert_eq!(https("https://schon-sicher/a".into()), "https://schon-sicher/a");
    }

    #[test]
    fn aktiver_skin_wird_bevorzugt() {
        let p = MinecraftProfile {
            id: "x".into(), name: "n".into(), capes: vec![],
            skins: vec![
                Texture { id: "1".into(), state: "INACTIVE".into(),
                          url: "alt".into(), variant: None },
                Texture { id: "2".into(), state: "ACTIVE".into(),
                          url: "neu".into(), variant: Some("CLASSIC".into()) },
            ],
        };
        assert_eq!(p.active_skin().unwrap().url, "neu");
    }
}
