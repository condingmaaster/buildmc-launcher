//! Skins und Capes verwalten.
//!
//! Alles ueber die offizielle Schnittstelle von `api.minecraftservices.com`
//! und damit an das Minecraft-Zugriffstoken gebunden.
//!
//! Zwei Grenzen, die nicht am Launcher liegen:
//!
//! * **Capes lassen sich nicht hochladen.** Mojang vergibt sie; die
//!   Schnittstelle kann nur zwischen den bereits besessenen auswaehlen.
//!   Ein eigenes Cape gibt es nur ueber Client-Mods, und dann sehen es
//!   auch nur Spieler mit derselben Mod.
//! * **Eine laufende Instanz uebernimmt die Aenderung nicht.** Der Client
//!   haelt die Skins der Sitzung im Zwischenspeicher und fragt sie nicht
//!   erneut ab. Die Aenderung wird beim naechsten Beitritt sichtbar. Einen
//!   offiziellen Weg, sie in einen laufenden Client zu schieben, gibt es
//!   nicht.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http;

use super::profile::MinecraftProfile;

const BASE: &str = "https://api.minecraftservices.com/minecraft/profile";

/// Armbreite des Modells. Betrifft nur die Arme, nicht den Kopf.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkinVariant {
    /// Vier Bildpunkte breite Arme -- das urspruengliche Modell.
    Classic,
    /// Drei Bildpunkte breite Arme.
    Slim,
}

impl SkinVariant {
    fn as_str(self) -> &'static str {
        match self {
            SkinVariant::Classic => "classic",
            SkinVariant::Slim => "slim",
        }
    }
}

/// Ein Cape, das dem Konto gehoert.
#[derive(Debug, Clone, Serialize)]
pub struct OwnedCape {
    pub id: String,
    pub url: String,
    pub active: bool,
    /// Ableitung aus der Adresse, damit die Oberflaeche einen Namen zeigt.
    pub name: String,
}

/// Setzt einen Skin aus einer Bildadresse.
pub async fn set_from_url(token: &str, url: &str, variant: SkinVariant) -> Result<()> {
    let body = serde_json::json!({ "variant": variant.as_str(), "url": url });
    let response = http::client()
        .post(format!("{BASE}/skins"))
        .bearer_auth(token)
        .json(&body)
        .send()
        .await?;
    check(response, "Skin setzen").await
}

/// Laedt eine Skin-Datei hoch.
///
/// Erwartet ein PNG mit 64x64 oder 64x32 Bildpunkten -- alles andere lehnt
/// Mojang ab. Geprueft wird hier vorab, damit die Meldung verstaendlich
/// bleibt statt als roher Fehler vom Dienst zu kommen.
pub async fn upload(token: &str, png: Vec<u8>, variant: SkinVariant) -> Result<()> {
    let (w, h) = png_size(&png)
        .ok_or_else(|| Error::other("Die Datei ist kein PNG"))?;
    if !(w == 64 && (h == 64 || h == 32)) {
        return Err(Error::other(format!(
            "Ein Skin muss 64x64 oder 64x32 Bildpunkte groß sein, diese Datei ist {w}x{h}"
        )));
    }

    let form = reqwest::multipart::Form::new()
        .text("variant", variant.as_str())
        .part(
            "file",
            reqwest::multipart::Part::bytes(png)
                .file_name("skin.png")
                .mime_str("image/png")
                .map_err(|e| Error::other(e.to_string()))?,
        );

    let response = http::client()
        .post(format!("{BASE}/skins"))
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await?;
    check(response, "Skin hochladen").await
}

/// Setzt den Skin auf das Standardmodell zurueck.
pub async fn reset(token: &str) -> Result<()> {
    let response = http::client()
        .delete(format!("{BASE}/skins/active"))
        .bearer_auth(token)
        .send()
        .await?;
    check(response, "Skin zurücksetzen").await
}

/// Waehlt eines der besessenen Capes aus.
pub async fn set_cape(token: &str, cape_id: &str) -> Result<()> {
    let response = http::client()
        .put(format!("{BASE}/capes/active"))
        .bearer_auth(token)
        .json(&serde_json::json!({ "capeId": cape_id }))
        .send()
        .await?;
    check(response, "Cape auswählen").await
}

/// Blendet das Cape aus.
pub async fn hide_cape(token: &str) -> Result<()> {
    let response = http::client()
        .delete(format!("{BASE}/capes/active"))
        .bearer_auth(token)
        .send()
        .await?;
    check(response, "Cape ausblenden").await
}

/// Die Capes des Kontos, aus einem bereits geholten Profil.
pub fn owned_capes(profile: &MinecraftProfile) -> Vec<OwnedCape> {
    profile
        .capes
        .iter()
        .map(|c| OwnedCape {
            id: c.id.clone(),
            url: c.url.clone(),
            active: c.state.eq_ignore_ascii_case("ACTIVE"),
            name: cape_name(&c.url),
        })
        .collect()
}

/// Mojang liefert keinen Namen zum Cape, nur eine Adresse. Der letzte
/// Pfadabschnitt ist ein Hash, taugt also nicht. Bleibt eine Nummerierung,
/// solange die Oberflaeche das Bild ohnehin zeigt.
fn cape_name(url: &str) -> String {
    let tail = url.rsplit('/').next().unwrap_or("");
    format!("Cape {}", &tail[..tail.len().min(6)])
}

/// Liest Breite und Hoehe aus dem IHDR-Block eines PNG.
fn png_size(data: &[u8]) -> Option<(u32, u32)> {
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let w = u32::from_be_bytes(data[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(data[20..24].try_into().ok()?);
    Some((w, h))
}

async fn check(response: reqwest::Response, was: &str) -> Result<()> {
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(Error::other("Sitzung abgelaufen, bitte neu anmelden"));
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(Error::other(
            "Zu viele Änderungen in kurzer Zeit. Mojang begrenzt das — später erneut versuchen.",
        ));
    }
    let detail = response.text().await.unwrap_or_default();
    Err(Error::other(format!("{was} fehlgeschlagen: {status} {detail}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kleinstes gueltiges PNG-Kopfstueck mit gesetzter Groesse.
    fn png_header(w: u32, h: u32) -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
        v.extend_from_slice(&[0, 0, 0, 13]);       // Laenge IHDR
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&w.to_be_bytes());
        v.extend_from_slice(&h.to_be_bytes());
        v
    }

    #[test]
    fn png_groesse_wird_gelesen() {
        assert_eq!(png_size(&png_header(64, 64)), Some((64, 64)));
        assert_eq!(png_size(&png_header(64, 32)), Some((64, 32)));
        assert_eq!(png_size(b"kein png"), None);
    }

    #[test]
    fn variante_heisst_wie_mojang_sie_nennt() {
        assert_eq!(SkinVariant::Classic.as_str(), "classic");
        assert_eq!(SkinVariant::Slim.as_str(), "slim");
    }

    #[test]
    fn cape_bekommt_einen_anzeigenamen() {
        let n = cape_name("https://textures.minecraft.net/texture/abcdef1234567890");
        assert!(n.starts_with("Cape "));
        assert!(n.len() <= 11);
    }
}
