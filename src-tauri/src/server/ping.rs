//! Server-Status abfragen.
//!
//! Minecraft beantwortet vor dem Beitritt eine Statusabfrage. Das Format
//! ist ein eigenes: jedes Paket beginnt mit seiner Laenge als *VarInt* --
//! einer Ganzzahl variabler Laenge, bei der sieben Bit je Byte den Wert
//! tragen und das achte anzeigt, ob ein weiteres Byte folgt. Kleine Werte
//! belegen so ein Byte statt vier.
//!
//! Der Ablauf ist festgelegt:
//!
//! 1. *Handshake* mit Adresse, Port und der Absicht "Status"
//! 2. *Status Request* -- ein leeres Paket
//! 3. Antwort als JSON mit Beschreibung, Spielerzahl und Fassung

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const TIMEOUT: Duration = Duration::from_secs(5);

/// Was die Oberflaeche anzeigt.
#[derive(Debug, Clone, Serialize)]
pub struct ServerStatus {
    pub online: bool,
    pub players_online: u32,
    pub players_max: u32,
    pub version: String,
    pub motd: String,
    pub latency_ms: u64,
    /// Serversymbol als Data-URI, sofern der Server eines sendet.
    pub favicon: Option<String>,
}

impl ServerStatus {
    fn offline() -> Self {
        Self {
            online: false, players_online: 0, players_max: 0,
            version: String::new(), motd: String::new(),
            latency_ms: 0, favicon: None,
        }
    }
}

/* ---- Antwortformat ----------------------------------------------------- */

#[derive(Debug, Deserialize)]
struct StatusResponse {
    #[serde(default)]
    version: Option<VersionInfo>,
    #[serde(default)]
    players: Option<Players>,
    #[serde(default)]
    description: Option<serde_json::Value>,
    #[serde(default)]
    favicon: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VersionInfo {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct Players {
    #[serde(default)]
    online: u32,
    #[serde(default)]
    max: u32,
}

/* ---- VarInt ------------------------------------------------------------ */

/// Schreibt eine Zahl im VarInt-Format.
pub fn write_varint(value: i32, out: &mut Vec<u8>) {
    let mut v = value as u32;
    loop {
        let mut byte = (v & 0x7F) as u8;
        v >>= 7;
        if v != 0 {
            byte |= 0x80; // weiteres Byte folgt
        }
        out.push(byte);
        if v == 0 {
            break;
        }
    }
}

/// Liest eine Zahl im VarInt-Format.
fn read_varint(stream: &mut impl Read) -> Result<i32> {
    let mut result: i32 = 0;
    let mut shift = 0;
    loop {
        let mut byte = [0u8; 1];
        stream.read_exact(&mut byte)?;
        result |= ((byte[0] & 0x7F) as i32) << shift;
        if byte[0] & 0x80 == 0 {
            return Ok(result);
        }
        shift += 7;
        if shift >= 35 {
            return Err(Error::other("Ungültige Zahl im Serverpaket"));
        }
    }
}

fn write_string(s: &str, out: &mut Vec<u8>) {
    write_varint(s.len() as i32, out);
    out.extend_from_slice(s.as_bytes());
}

/// Packt Inhalt in ein Paket: Laenge, Kennung, Daten.
fn packet(id: i32, body: &[u8]) -> Vec<u8> {
    let mut inner = Vec::new();
    write_varint(id, &mut inner);
    inner.extend_from_slice(body);

    let mut out = Vec::new();
    write_varint(inner.len() as i32, &mut out);
    out.extend_from_slice(&inner);
    out
}

/* ---- Abfrage ----------------------------------------------------------- */

/// Fragt einen Server ab. Antwortet er nicht, gilt er als offline --
/// das ist kein Fehler, sondern eine Auskunft.
pub fn ping(address: &str) -> ServerStatus {
    match query(address) {
        Ok(status) => status,
        Err(_) => ServerStatus::offline(),
    }
}

fn query(address: &str) -> Result<ServerStatus> {
    let (host, port) = super::split_address(address);
    let started = Instant::now();

    let target = format!("{host}:{port}");
    let addr = target
        .to_socket_addrs_first()
        .ok_or_else(|| Error::other("Adresse nicht auflösbar"))?;

    let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT)?;
    stream.set_read_timeout(Some(TIMEOUT))?;
    stream.set_write_timeout(Some(TIMEOUT))?;

    // 1. Handshake. Protokollnummer -1 bedeutet "nur fragen" und wird von
    //    jeder Serverfassung angenommen.
    let mut body = Vec::new();
    write_varint(-1, &mut body);
    write_string(&host, &mut body);
    body.extend_from_slice(&port.to_be_bytes());
    write_varint(1, &mut body); // naechster Zustand: Status
    stream.write_all(&packet(0x00, &body))?;

    // 2. Statusanfrage, leer.
    stream.write_all(&packet(0x00, &[]))?;
    stream.flush()?;

    // 3. Antwort
    let _length = read_varint(&mut stream)?;
    let id = read_varint(&mut stream)?;
    if id != 0x00 {
        return Err(Error::other("Unerwartete Antwort des Servers"));
    }

    let json_len = read_varint(&mut stream)? as usize;
    // Eine Antwort von mehr als einem Megabyte ist kein Serverstatus.
    if json_len > 1_048_576 {
        return Err(Error::other("Antwort des Servers ist unplausibel groß"));
    }
    let mut buffer = vec![0u8; json_len];
    stream.read_exact(&mut buffer)?;

    let latency_ms = started.elapsed().as_millis() as u64;
    let parsed: StatusResponse = serde_json::from_slice(&buffer)?;

    Ok(ServerStatus {
        online: true,
        players_online: parsed.players.as_ref().map(|p| p.online).unwrap_or(0),
        players_max: parsed.players.as_ref().map(|p| p.max).unwrap_or(0),
        version: parsed.version.map(|v| v.name).unwrap_or_default(),
        motd: parsed.description.map(|d| flatten_motd(&d)).unwrap_or_default(),
        latency_ms,
        favicon: parsed.favicon.filter(|f| f.starts_with("data:image")),
    })
}

/// Die Beschreibung kann Text, ein Objekt oder ein Baum aus Teilstuecken
/// sein -- je nach Serverfassung. Gesucht ist die reine Zeichenfolge.
fn flatten_motd(value: &serde_json::Value) -> String {
    let mut out = String::new();
    collect_text(value, &mut out);
    // Farbcodes entfernen: Paragraphenzeichen plus ein Zeichen.
    let mut clean = String::with_capacity(out.len());
    let mut chars = out.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            clean.push(c);
        }
    }
    clean.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn collect_text(value: &serde_json::Value, out: &mut String) {
    match value {
        serde_json::Value::String(s) => out.push_str(s),
        serde_json::Value::Array(items) => items.iter().for_each(|i| collect_text(i, out)),
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(t)) = map.get("text") {
                out.push_str(t);
            }
            if let Some(extra) = map.get("extra") {
                collect_text(extra, out);
            }
        }
        _ => {}
    }
}

/// Kleine Hilfe, um die erste aufloesbare Adresse zu bekommen.
trait FirstAddr {
    fn to_socket_addrs_first(&self) -> Option<std::net::SocketAddr>;
}

impl FirstAddr for String {
    fn to_socket_addrs_first(&self) -> Option<std::net::SocketAddr> {
        use std::net::ToSocketAddrs;
        self.to_socket_addrs().ok()?.next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_kleine_werte_belegen_ein_byte() {
        let mut v = Vec::new();
        write_varint(0, &mut v);
        assert_eq!(v, vec![0x00]);

        v.clear();
        write_varint(127, &mut v);
        assert_eq!(v, vec![0x7F]);
    }

    #[test]
    fn varint_groessere_werte_werden_aufgeteilt() {
        let mut v = Vec::new();
        write_varint(128, &mut v);
        assert_eq!(v, vec![0x80, 0x01]);

        v.clear();
        write_varint(2097151, &mut v);
        assert_eq!(v, vec![0xFF, 0xFF, 0x7F]);
    }

    #[test]
    fn varint_minus_eins_ist_fuenf_byte() {
        // -1 wird als vorzeichenlose Zahl geschrieben und braucht damit
        // die volle Laenge -- genau das sendet der Handshake.
        let mut v = Vec::new();
        write_varint(-1, &mut v);
        assert_eq!(v, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    }

    #[test]
    fn varint_hin_und_zurueck() {
        for wert in [0, 1, 127, 128, 255, 2097151, i32::MAX] {
            let mut v = Vec::new();
            write_varint(wert, &mut v);
            let gelesen = read_varint(&mut v.as_slice()).unwrap();
            assert_eq!(gelesen, wert, "bei {wert}");
        }
    }

    #[test]
    fn paket_traegt_laenge_und_kennung() {
        let p = packet(0x00, b"abc");
        // Laenge 4 (Kennung + drei Zeichen), dann Kennung 0, dann Inhalt.
        assert_eq!(p, vec![0x04, 0x00, b'a', b'b', b'c']);
    }

    #[test]
    fn beschreibung_als_reiner_text() {
        let v = serde_json::json!("Willkommen");
        assert_eq!(flatten_motd(&v), "Willkommen");
    }

    #[test]
    fn beschreibung_als_baum_wird_zusammengesetzt() {
        let v = serde_json::json!({
            "text": "Mein ",
            "extra": [{ "text": "Server" }, { "text": "!" }]
        });
        assert_eq!(flatten_motd(&v), "Mein Server!");
    }

    #[test]
    fn farbcodes_werden_entfernt() {
        let v = serde_json::json!("§aGrün §lfett");
        assert_eq!(flatten_motd(&v), "Grün fett");
    }

    #[test]
    fn nicht_erreichbarer_server_gilt_als_offline() {
        // Port 1 ist reserviert und nimmt nichts an.
        let s = ping("127.0.0.1:1");
        assert!(!s.online);
        assert_eq!(s.players_online, 0);
    }
}
