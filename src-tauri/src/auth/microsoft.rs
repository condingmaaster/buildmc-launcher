//! Anmeldung mit einem Microsoft-Konto.
//!
//! Der Weg zum Minecraft-Token führt über vier Dienste, jeder mit eigenem
//! Format:
//!
//! 1. **Microsoft-Konto** über `login.live.com` -- den älteren Live-Weg,
//!    nicht den Azure-Weg. Er kommt mit der vorgegebenen Rückleitung
//!    `oauth20_desktop.srf` aus, braucht also keinen eigenen Dienst, und
//!    verwendet den Geltungsbereich `service::user.auth.xboxlive.com::MBI_SSL`.
//!    Zwei Wege stehen offen: ein Anmeldefenster mit Rückleitung oder das
//!    Gerätecode-Verfahren über `oauth20_connect.srf`.
//!
//!    Dass es dieser Weg ist, stammt nicht aus einer Annahme: die Adressen
//!    stehen so im Binary der Fassung 0.1.3.
//! 2. **Xbox Live** -- tauscht das Microsoft-Token gegen ein Xbox-Token.
//! 3. **XSTS** -- autorisiert es für Minecraft. Hier kommen die
//!    aussagekräftigsten Fehler: kein Xbox-Konto, Kindersicherung, Region.
//! 4. **Minecraft** -- liefert schliesslich das Spiel-Token.
//!
//! Tokens werden nirgends protokolliert. Fehlermeldungen führen nur
//! Kennungen, keine Werte.

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::http;

const AUTHORIZE_URL: &str = "https://login.live.com/oauth20_authorize.srf";
/// Von Microsoft vorgegebene Rückleitung für Desktop-Anwendungen. Auf sie
/// landet der Browser mit dem Code in der Adresszeile; ein eigener Dienst
/// ist dafür nicht nötig.
const REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";
const DEVICE_CODE_URL: &str = "https://login.live.com/oauth20_connect.srf";
const TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";
const XBL_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MC_LOGIN_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";

/// Der Geltungsbereich des Live-Wegs. Er schliesst das Erneuerungstoken
/// mit ein -- ohne das müsste man sich täglich neu anmelden.
const SCOPE: &str = "service::user.auth.xboxlive.com::MBI_SSL";

/* ---- Anmeldefenster ------------------------------------------------------ */

/// Baut die Adresse, die im Anmeldefenster geöffnet wird.
///
/// `prompt=select_account` sorgt dafür, dass bei mehreren Konten gefragt
/// wird, statt stillschweigend das zuletzt benutzte zu nehmen.
pub fn authorize_url(client_id: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?prompt=select_account&client_id={client_id}\
         &response_type=code&redirect_uri={REDIRECT_URI}&scope={}",
        urlencode(SCOPE)
    )
}

/// Zieht den Code aus der Adresse, auf der das Fenster landet.
///
/// Bricht der Nutzer ab, steht dort stattdessen `error=` -- das ist kein
/// Fehlschlag, sondern eine Entscheidung, und wird als solche gemeldet.
pub fn code_from_redirect(url: &str) -> Result<String> {
    if !url.starts_with(REDIRECT_URI) {
        return Err(Error::other("Unerwartete Rückleitung der Anmeldung"));
    }
    if let Some(code) = query_value(url, "code") {
        return Ok(code);
    }
    match query_value(url, "error") {
        Some(e) if e == "access_denied" => Err(Error::other("Die Anmeldung wurde abgebrochen.")),
        Some(e) => Err(Error::other(format!("Die Anmeldung schlug fehl ({e})."))),
        None => Err(Error::other("Die Rückleitung enthielt keinen Anmeldecode.")),
    }
}

fn query_value(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=')?;
        if k == key {
            return Some(urldecode(v));
        }
    }
    None
}

fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn urldecode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(b) => { out.push(b); i += 3; }
                    Err(_) => { out.push(bytes[i]); i += 1; }
                }
            }
            b'+' => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Tauscht den Code aus der Rückleitung gegen ein Token.
pub async fn exchange_code(client_id: &str, code: &str) -> Result<Account> {
    let response = http::client()
        .post(TOKEN_URL)
        .form(&[
            ("client_id", client_id),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", REDIRECT_URI),
            ("scope", SCOPE),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(describe_ms_error(&response.text().await.unwrap_or_default()));
    }
    let token: TokenResponse = response.json().await?;
    complete_login(&token.access_token, &token.refresh_token).await
}

/* ---- Gerätecode --------------------------------------------------------- */

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeviceCodeStart {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Sekunden, die der Code gilt.
    pub expires_in: u64,
    /// Mindestabstand zwischen zwei Abfragen, in Sekunden.
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// Beginnt die Anmeldung.
pub async fn start_device_code(client_id: &str) -> Result<DeviceCodeStart> {
    let response = http::client()
        .post(DEVICE_CODE_URL)
        .form(&[("client_id", client_id), ("scope", SCOPE), ("response_type", "device_code")])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(describe_ms_error(&response.text().await.unwrap_or_default()));
    }
    Ok(response.json().await?)
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    // Microsoft nennt hier eine Gueltigkeit, die uns nicht betrifft:
    // massgeblich ist die des Minecraft-Tokens weiter hinten in der Kette.
}

#[derive(Debug, Deserialize)]
struct OAuthError {
    error: String,
    #[serde(default)]
    error_description: String,
}

/// Das Ergebnis einer Abfrage.
pub enum PollOutcome {
    /// Der Nutzer hat den Code noch nicht eingegeben.
    Pending,
    /// Zu schnell gefragt -- der Abstand muss wachsen.
    SlowDown,
    Ready(Box<Account>),
}

/// Fragt einmal nach, ob der Nutzer bestätigt hat.
pub async fn poll_device_code(client_id: &str, device_code: &str) -> Result<PollOutcome> {
    let response = http::client()
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ("client_id", client_id),
            ("device_code", device_code),
        ])
        .send()
        .await?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();

    if !status.is_success() {
        let parsed: OAuthError = serde_json::from_str(&body)
            .unwrap_or(OAuthError { error: "unknown".into(), error_description: String::new() });
        return match parsed.error.as_str() {
            // Diese beiden sind kein Fehler, sondern der normale Verlauf.
            "authorization_pending" => Ok(PollOutcome::Pending),
            "slow_down" => Ok(PollOutcome::SlowDown),
            "expired_token" => Err(Error::other(
                "Der Anmeldecode ist abgelaufen. Bitte neu beginnen.",
            )),
            "authorization_declined" => {
                Err(Error::other("Die Anmeldung wurde abgelehnt."))
            }
            other => Err(Error::other(format!(
                "Anmeldung fehlgeschlagen ({other}): {}",
                parsed.error_description
            ))),
        };
    }

    let token: TokenResponse = serde_json::from_str(&body)?;
    let account = complete_login(&token.access_token, &token.refresh_token).await?;
    Ok(PollOutcome::Ready(Box::new(account)))
}

/// Muss das Konto vor dem Start erneuert werden?
///
/// Reiner Entscheid, ohne Netzwerk -- damit lassen sich die beiden Faelle
/// (Offline-Konto, noch gueltiges Token) ohne Mock testen.
fn braucht_erneuerung(account: &super::Account) -> bool {
    !account.is_offline() && !account.token_valid()
}

/// Erneuert das Konto vor dem Start, falls noetig -- sonst unveraendert.
///
/// Das Spiel selbst kann ein abgelaufenes Zugriffstoken nicht erneuern: es
/// bekommt beim Start nur den fertigen Token, nie den refresh_token. Ohne
/// diese Pruefung startet Minecraft mit einem toten Token, und jede
/// Mojang-Anfrage danach -- Profilbild, Realms, sogar der Serverbeitritt --
/// schlaegt mit 401 fehl, mitten im laufenden Spiel statt vorher sichtbar.
pub async fn ensure_fresh(account: &super::Account) -> Result<super::Account> {
    if !braucht_erneuerung(account) {
        return Ok(account.clone());
    }

    let client_id = crate::ai::keys::ms_client_id().ok_or_else(|| {
        Error::other(
            "Das Zugriffstoken ist abgelaufen, aber es ist keine              Azure-Anwendungskennung hinterlegt. Unter Einstellungen →              Konten erneut bei Microsoft anmelden.",
        )
    })?;

    let mut fresh = refresh(&client_id, &account.refresh_token).await?;
    // Kennung und Aktivzustand des bisherigen Eintrags behalten -- refresh()
    // kennt nur den neuen Token, nicht die Stelle in der Kontenliste.
    fresh.id = account.id.clone();
    fresh.active = account.active;
    Ok(fresh)
}

/// Holt mit dem Erneuerungstoken ein frisches Spiel-Token.
pub async fn refresh(client_id: &str, refresh_token: &str) -> Result<Account> {
    let response = http::client()
        .post(TOKEN_URL)
        .form(&[
            ("grant_type", "refresh_token"),
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("scope", SCOPE),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(Error::other(
            "Die Sitzung liess sich nicht erneuern. Bitte neu anmelden.",
        ));
    }

    let token: TokenResponse = response.json().await?;
    let next_refresh = if token.refresh_token.is_empty() {
        refresh_token.to_string()
    } else {
        token.refresh_token.clone()
    };
    complete_login(&token.access_token, &next_refresh).await
}

/* ---- Xbox Live und XSTS -------------------------------------------------- */

#[derive(Debug, Deserialize)]
struct XboxResponse {
    #[serde(rename = "Token")]
    token: String,
    #[serde(rename = "DisplayClaims")]
    display_claims: DisplayClaims,
}

#[derive(Debug, Deserialize)]
struct DisplayClaims {
    xui: Vec<XuiClaim>,
}

#[derive(Debug, Deserialize)]
struct XuiClaim {
    uhs: String,
}

async fn xbox_live(ms_token: &str) -> Result<XboxResponse> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            // Beim Live-Weg wird das Token unveraendert gereicht. Das
            // Praefix "d=" gehoert zum Azure-Weg und wuerde hier abgelehnt.
            "RpsTicket": ms_token,
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT",
    });

    let response = http::client().post(XBL_URL).json(&body).send().await?;
    if !response.status().is_success() {
        return Err(Error::other(
            "Xbox Live hat die Anmeldung abgelehnt. Besitzt das Konto ein Xbox-Profil?",
        ));
    }
    Ok(response.json().await?)
}

async fn xsts(xbl_token: &str) -> Result<XboxResponse> {
    let body = serde_json::json!({
        "Properties": { "SandboxId": "RETAIL", "UserTokens": [xbl_token] },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT",
    });

    let response = http::client().post(XSTS_URL).json(&body).send().await?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(describe_xsts_error(&text));
    }
    Ok(response.json().await?)
}

/// XSTS meldet die aussagekräftigsten Fehler -- als Zahl. Ohne Übersetzung
/// stünde der Nutzer vor "XErr: 2148916233" und wüsste nichts damit anzufangen.
pub fn describe_xsts_error(body: &str) -> Error {
    let code = body
        .split("\"XErr\":")
        .nth(1)
        .and_then(|rest| rest.split(|c: char| !c.is_ascii_digit()).find(|s| !s.is_empty()))
        .unwrap_or("");

    Error::other(match code {
        "2148916233" => "Dieses Microsoft-Konto hat kein Xbox-Profil. \
                         Lege zuerst eines auf xbox.com an.".to_string(),
        "2148916235" => "Xbox Live ist in der Region dieses Kontos nicht verfügbar.".to_string(),
        "2148916236" | "2148916237" => "Für dieses Konto ist eine Altersprüfung nötig.".to_string(),
        "2148916238" => "Das Konto gehört zu einem Kind und muss einer Familie \
                         zugeordnet sein.".to_string(),
        "" => "Die Autorisierung bei Xbox wurde abgelehnt.".to_string(),
        other => format!("Die Autorisierung bei Xbox wurde abgelehnt (Code {other})."),
    })
}

fn describe_ms_error(body: &str) -> Error {
    if body.contains("unauthorized_client") || body.contains("invalid_client") {
        return Error::other(
            "Microsoft kennt diese Anwendungskennung nicht oder erlaubt das \
             Gerätecode-Verfahren nicht. Prüfe die Registrierung in Azure.",
        );
    }
    Error::other("Die Anmeldung bei Microsoft konnte nicht begonnen werden.")
}

/* ---- Minecraft ----------------------------------------------------------- */

#[derive(Debug, Deserialize)]
struct MinecraftToken {
    access_token: String,
    #[serde(default)]
    expires_in: i64,
}

/// Setzt den Kopf zusammen, den Minecraft erwartet.
pub fn identity_token(user_hash: &str, xsts_token: &str) -> String {
    format!("XBL3.0 x={user_hash};{xsts_token}")
}

use super::Account;

/// Führt die Kette vom Microsoft-Token bis zum fertigen Konto.
async fn complete_login(ms_token: &str, refresh_token: &str) -> Result<Account> {
    let xbl = xbox_live(ms_token).await?;
    let user_hash = xbl
        .display_claims
        .xui
        .first()
        .map(|c| c.uhs.clone())
        .ok_or_else(|| Error::other("Xbox Live lieferte keine Kontokennung"))?;

    let xsts = xsts(&xbl.token).await?;

    let response = http::client()
        .post(MC_LOGIN_URL)
        .json(&serde_json::json!({
            "identityToken": identity_token(&user_hash, &xsts.token),
        }))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(Error::other(
            "Minecraft hat die Anmeldung abgelehnt. Besitzt dieses Konto das Spiel?",
        ));
    }

    let mc: MinecraftToken = response.json().await?;
    let profile = super::profile::fetch_own(&mc.access_token).await?;

    // Den Skin gleich mitnehmen, damit die Kontoanzeige sofort stimmt.
    let skin_url = profile.active_skin().map(|s| to_https(&s.url));
    let cape_url = profile.active_cape().map(|c| to_https(&c.url));

    Ok(Account {
        id: uuid::Uuid::new_v4().to_string(),
        username: profile.name.clone(),
        uuid: profile.dashed_id(),
        account_type: "microsoft".into(),
        active: false,
        access_token: mc.access_token,
        refresh_token: refresh_token.to_string(),
        expires_at: chrono::Utc::now().timestamp() + mc.expires_in.max(0),
        skin_url,
        cape_url,
    })
}

fn to_https(url: &str) -> String {
    match url.strip_prefix("http://") {
        Some(rest) => format!("https://{rest}"),
        None => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn konto(offline: bool, expires_in_sek: i64) -> super::super::Account {
        super::super::Account {
            account_type: if offline { "offline".into() } else { "microsoft".into() },
            access_token: "t".into(),
            expires_at: chrono::Utc::now().timestamp() + expires_in_sek,
            ..Default::default()
        }
    }

    #[test]
    fn abgelaufenes_konto_braucht_erneuerung() {
        assert!(braucht_erneuerung(&konto(false, -10)));
    }

    #[test]
    fn gueltiges_konto_braucht_keine_erneuerung() {
        assert!(!braucht_erneuerung(&konto(false, 3600)));
    }

    #[test]
    fn kurz_vor_ablauf_zaehlt_schon_als_erneuerungsbeduerftig() {
        // Dieselbe Sicherheitsspanne wie Account::token_valid() -- sonst
        // laeuft das Token mitten im Start ab.
        assert!(braucht_erneuerung(&konto(false, 30)));
    }

    #[test]
    fn offline_konten_werden_nie_als_erneuerungsbeduerftig_markiert() {
        // Sie haben gar keinen refresh_token -- ein Erneuerungsversuch
        // wuerde nur mit einem unverstaendlichen Fehler enden.
        assert!(!braucht_erneuerung(&konto(true, -999999)));
    }

    #[test]
    fn identitaetskopf_hat_das_erwartete_format() {
        assert_eq!(identity_token("abc123", "TOKEN"), "XBL3.0 x=abc123;TOKEN");
    }

    #[test]
    fn xsts_fehler_werden_uebersetzt() {
        let ohne_profil = describe_xsts_error(r#"{"XErr":2148916233,"Message":""}"#);
        assert!(ohne_profil.to_string().contains("Xbox-Profil"));

        let kind = describe_xsts_error(r#"{"XErr":2148916238}"#);
        assert!(kind.to_string().contains("Kind"));

        let region = describe_xsts_error(r#"{"XErr":2148916235}"#);
        assert!(region.to_string().contains("Region"));
    }

    #[test]
    fn unbekannter_xsts_code_nennt_die_zahl() {
        let e = describe_xsts_error(r#"{"XErr":1234567}"#).to_string();
        assert!(e.contains("1234567"), "die Zahl hilft beim Nachschlagen");
    }

    #[test]
    fn fehlende_kennung_ergibt_eine_allgemeine_meldung() {
        let e = describe_xsts_error("{}").to_string();
        assert!(e.contains("abgelehnt"));
        assert!(!e.contains("Code"), "ohne Zahl keine leere Klammer");
    }

    #[test]
    fn falsche_anwendungskennung_wird_benannt() {
        let e = describe_ms_error(r#"{"error":"unauthorized_client"}"#).to_string();
        assert!(e.contains("Azure"), "der Nutzer muss wissen, wo er nachsieht");
    }

    #[test]
    fn skin_adresse_wird_auf_https_gehoben() {
        assert_eq!(to_https("http://textures.minecraft.net/a"),
                   "https://textures.minecraft.net/a");
        assert_eq!(to_https("https://schon/a"), "https://schon/a");
    }

    #[test]
    fn anmeldeadresse_nutzt_den_live_weg() {
        let url = authorize_url("MEINE-KENNUNG");
        assert!(url.starts_with("https://login.live.com/oauth20_authorize.srf"));
        assert!(url.contains("client_id=MEINE-KENNUNG"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("prompt=select_account"), "bei mehreren Konten fragen");
        // Der alte Geltungsbereich, kodiert.
        assert!(url.contains("MBI_SSL"));
    }

    #[test]
    fn code_wird_aus_der_rueckleitung_gelesen() {
        let url = "https://login.live.com/oauth20_desktop.srf?code=M.C123_BAY.abc";
        assert_eq!(code_from_redirect(url).unwrap(), "M.C123_BAY.abc");
    }

    #[test]
    fn abbruch_ist_kein_fehlschlag_sondern_eine_entscheidung() {
        let url = "https://login.live.com/oauth20_desktop.srf?error=access_denied";
        let e = code_from_redirect(url).unwrap_err().to_string();
        assert!(e.contains("abgebrochen"));
    }

    #[test]
    fn fremde_rueckleitung_wird_abgewiesen() {
        assert!(code_from_redirect("https://boese.example/?code=x").is_err());
    }

    #[test]
    fn kodierung_und_entschluesselung() {
        assert_eq!(urlencode("a b"), "a%20b");
        assert_eq!(urlencode("service::x"), "service%3A%3Ax");
        assert_eq!(urldecode("a%20b"), "a b");
        assert_eq!(urldecode("M.C123_BAY.abc"), "M.C123_BAY.abc");
    }

    #[test]
    fn abstand_hat_einen_standardwert() {
        // Microsoft laesst `interval` gelegentlich weg.
        let d: DeviceCodeStart = serde_json::from_str(
            r#"{"device_code":"d","user_code":"ABCD","verification_uri":"https://x",
                "expires_in":900}"#,
        ).unwrap();
        assert_eq!(d.interval, 5);
    }
}
