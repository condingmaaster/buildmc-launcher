//! Offene Spiele im lokalen Netz finden.
//!
//! Wer in Minecraft eine Welt "fuer LAN oeffnet", sendet viermal je
//! Sekunde eine Nachricht an die Multicast-Gruppe 224.0.2.60, Port 4445.
//! Sie enthaelt Name und Port in einem sehr einfachen Format:
//!
//! ```text
//! [MOTD]Jonas Welt[/MOTD][AD]25565[/AD]
//! ```
//!
//! Zugehoert wird eine kurze Zeit lang; jede Adresse zaehlt nur einmal,
//! sonst stuende dieselbe Welt viermal in der Liste.

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::Result;

const GROUP: Ipv4Addr = Ipv4Addr::new(224, 0, 2, 60);
const PORT: u16 = 4445;

#[derive(Debug, Clone, Serialize)]
pub struct LanServer {
    pub address: String,
    pub port: u16,
    pub motd: String,
    pub discovered_at: String,
}

/// Liest Name und Port aus einer Ankuendigung.
///
/// Eine Nachricht ohne Port ist unbrauchbar -- ohne ihn liesse sich nicht
/// beitreten. Ein fehlender Name ist dagegen hinnehmbar.
pub fn parse_announcement(text: &str) -> Option<(String, u16)> {
    let port = between(text, "[AD]", "[/AD]")?.trim().parse().ok()?;
    let motd = between(text, "[MOTD]", "[/MOTD]")
        .map(|s| strip_colors(s.trim()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Offenes Spiel".into());
    Some((motd, port))
}

fn between<'a>(haystack: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let start = haystack.find(open)? + open.len();
    let rest = &haystack[start..];
    let end = rest.find(close)?;
    Some(&rest[..end])
}

/// Entfernt Farbcodes: Paragraphenzeichen plus das folgende Zeichen.
fn strip_colors(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out.trim().to_string()
}

/// Hoert eine Weile zu und meldet, was sich gezeigt hat.
pub fn discover(timeout_ms: u64) -> Result<Vec<LanServer>> {
    let socket = UdpSocket::bind(("0.0.0.0", PORT))?;
    socket.join_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED)?;
    // Kurz getaktet lesen, damit die Gesamtzeit eingehalten wird, auch
    // wenn nichts kommt.
    socket.set_read_timeout(Some(Duration::from_millis(250)))?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);
    let mut found: HashMap<String, LanServer> = HashMap::new();
    let mut buffer = [0u8; 1024];

    while Instant::now() < deadline {
        let Ok((len, from)) = socket.recv_from(&mut buffer) else { continue };
        let text = String::from_utf8_lossy(&buffer[..len]);

        let Some((motd, port)) = parse_announcement(&text) else { continue };
        let SocketAddr::V4(v4) = from else { continue };
        let address = v4.ip().to_string();

        found.entry(format!("{address}:{port}")).or_insert(LanServer {
            address,
            port,
            motd,
            discovered_at: crate::instance::now(),
        });
    }

    let _ = socket.leave_multicast_v4(&GROUP, &Ipv4Addr::UNSPECIFIED);

    let mut out: Vec<LanServer> = found.into_values().collect();
    out.sort_by(|a, b| a.motd.cmp(&b.motd));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ankuendigung_wird_gelesen() {
        let (motd, port) = parse_announcement("[MOTD]Jonas Welt[/MOTD][AD]25565[/AD]").unwrap();
        assert_eq!(motd, "Jonas Welt");
        assert_eq!(port, 25565);
    }

    #[test]
    fn farbcodes_verschwinden() {
        let (motd, _) = parse_announcement("[MOTD]§aGrüne §lWelt[/MOTD][AD]25565[/AD]").unwrap();
        assert_eq!(motd, "Grüne Welt");
    }

    #[test]
    fn ohne_port_unbrauchbar() {
        // Ohne Port liesse sich nicht beitreten.
        assert!(parse_announcement("[MOTD]Welt[/MOTD]").is_none());
        assert!(parse_announcement("[AD]keine Zahl[/AD]").is_none());
    }

    #[test]
    fn ohne_namen_bekommt_einen_platzhalter() {
        let (motd, port) = parse_announcement("[MOTD][/MOTD][AD]25566[/AD]").unwrap();
        assert_eq!(motd, "Offenes Spiel");
        assert_eq!(port, 25566);
    }

    #[test]
    fn fremder_text_wird_verworfen() {
        assert!(parse_announcement("irgendwas anderes").is_none());
    }
}
