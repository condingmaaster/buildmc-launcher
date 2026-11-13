//! Ein genuegsamer Leser fuer RSS und Atom.
//!
//! Bewusst ohne XML-Bibliothek: gebraucht werden vier Felder je Eintrag,
//! und die Feeds draussen sind unterschiedlich genug, dass ein strenger
//! Parser haeufiger scheitern wuerde als dieser hier. Wer einen Feed nicht
//! lesen kann, laesst ihn aus -- die Startseite darf daran nicht scheitern.
//!
//! Wichtig: uebernommen werden nur Ueberschrift, Kurzbeschreibung aus dem
//! Feed und der Verweis. Der Artikeltext bleibt bei der Quelle.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewsItem {
    pub title: String,
    pub link: String,
    /// Die vom Feed selbst gelieferte Kurzfassung, gekuerzt.
    pub summary: String,
    pub published: String,
    /// Woher der Eintrag stammt -- wird immer mit angezeigt.
    pub source: String,
}

/// Hoechstlaenge der Kurzbeschreibung.
///
/// Der Feed liefert teils ganze Absaetze. Angezeigt wird ein Anriss, der
/// zum Weiterlesen bei der Quelle fuehrt, nicht ihr Ersatz.
const SUMMARY_LIMIT: usize = 220;

/// Entscheidet die Kodierung und wandelt in Text.
///
/// Minecraft.net liefert UTF-16 -- ohne diese Behandlung kaeme nur
/// Zeichensalat an.
pub fn decode(raw: &[u8]) -> String {
    let utf16_le = raw.starts_with(&[0xFF, 0xFE]);
    let utf16_be = raw.starts_with(&[0xFE, 0xFF]);
    // Ohne Kennzeichnung: steht an zweiter Stelle eine Null, ist es UTF-16LE.
    let looks_utf16 = raw.len() > 1 && raw[1] == 0;

    if utf16_le || (looks_utf16 && !utf16_be) {
        let body = if utf16_le { &raw[2..] } else { raw };
        let units: Vec<u16> = body
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    if utf16_be {
        let units: Vec<u16> = raw[2..]
            .chunks_exact(2)
            .map(|c| u16::from_be_bytes([c[0], c[1]]))
            .collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(raw).into_owned()
}

/// Liest RSS und Atom.
pub fn parse(text: &str, source: &str) -> Vec<NewsItem> {
    let atom = text.contains("<entry");
    let (open, close) = if atom { ("<entry", "</entry>") } else { ("<item", "</item>") };

    let mut out = Vec::new();
    let mut rest = text;

    while let Some(start) = rest.find(open) {
        let after = &rest[start..];
        let Some(end) = after.find(close) else { break };
        let block = &after[..end];
        rest = &after[end + close.len()..];

        let title = tag(block, "title").unwrap_or_default();
        if title.is_empty() {
            continue;
        }

        let link = if atom { atom_link(block) } else { tag(block, "link") }
            .unwrap_or_default();

        let summary = tag(block, "description")
            .or_else(|| tag(block, "summary"))
            .unwrap_or_default();

        let published = tag(block, "pubDate")
            .or_else(|| tag(block, "updated"))
            .or_else(|| tag(block, "published"))
            .unwrap_or_default();

        out.push(NewsItem {
            title: clean(&title),
            link: clean(&link),
            summary: shorten(&clean(&summary)),
            published: clean(&published),
            source: source.to_string(),
        });
    }

    out
}

/// Inhalt des ersten Vorkommens eines Elements.
fn tag(block: &str, name: &str) -> Option<String> {
    let open = format!("<{name}");
    let close = format!("</{name}>");
    let start = block.find(&open)?;
    // Ueber das schliessende Zeichen des Starttags hinweg.
    let after_open = block[start..].find('>')? + start + 1;
    let end = block[after_open..].find(&close)? + after_open;
    Some(block[after_open..end].to_string())
}

/// Atom haengt den Verweis als Attribut an: `<link href="…"/>`.
fn atom_link(block: &str) -> Option<String> {
    let at = block.find("<link")?;
    let rest = &block[at..];
    let href = rest.find("href=")? + 5;
    let quote = rest[href..].chars().next()?;
    let value = &rest[href + 1..];
    let end = value.find(quote)?;
    Some(value[..end].to_string())
}

/// Entfernt CDATA, Markup und loest die wenigen Entitaeten auf, die in
/// Feeds tatsaechlich vorkommen.
fn clean(input: &str) -> String {
    let mut s = input.trim().to_string();

    if let Some(inner) = s.strip_prefix("<![CDATA[").and_then(|r| r.strip_suffix("]]>")) {
        s = inner.to_string();
    }

    // Markup heraus.
    let mut out = String::with_capacity(s.len());
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }

    // Doppelt kodierte Entitaeten kommen vor (&amp;amp;), deshalb zweimal.
    for _ in 0..2 {
        out = out
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&#39;", "'")
            .replace("&apos;", "'")
            .replace("&nbsp;", " ");
    }

    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Kuerzt auf Satzgrenze, sonst auf Wortgrenze.
fn shorten(s: &str) -> String {
    if s.chars().count() <= SUMMARY_LIMIT {
        return s.to_string();
    }
    let cut: String = s.chars().take(SUMMARY_LIMIT).collect();
    let at = cut.rfind(". ").map(|i| i + 1)
        .or_else(|| cut.rfind(' '))
        .unwrap_or(cut.len());
    format!("{}…", cut[..at].trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_wird_gelesen() {
        let xml = r#"<rss><channel><title>Kanal</title>
            <item><title>Erste Meldung</title><link>https://example.org/a</link>
            <description>Kurz erklärt.</description><pubDate>Mon, 22 Sep 2026</pubDate></item>
            <item><title>Zweite</title><link>https://example.org/b</link></item>
        </channel></rss>"#;
        let items = parse(xml, "Test");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].title, "Erste Meldung");
        assert_eq!(items[0].link, "https://example.org/a");
        assert_eq!(items[0].source, "Test");
        // Der Kanaltitel darf nicht als Eintrag durchgehen.
        assert!(!items.iter().any(|i| i.title == "Kanal"));
    }

    #[test]
    fn atom_wird_gelesen() {
        let xml = r#"<feed><title>Kanal</title>
            <entry><title>Fabric für 26.3</title>
            <link href="https://fabricmc.net/x" rel="alternate"/>
            <summary>Neue Fassung.</summary><updated>2026-09-01</updated></entry>
        </feed>"#;
        let items = parse(xml, "Fabric");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Fabric für 26.3");
        assert_eq!(items[0].link, "https://fabricmc.net/x");
    }

    #[test]
    fn utf16_wird_erkannt() {
        let text = "<rss><item><title>Hallo</title></item></rss>";
        let mut raw = vec![0xFF, 0xFE];
        for unit in text.encode_utf16() {
            raw.extend_from_slice(&unit.to_le_bytes());
        }
        let decoded = decode(&raw);
        assert!(decoded.contains("Hallo"));
        assert_eq!(parse(&decoded, "X")[0].title, "Hallo");
    }

    #[test]
    fn markup_und_entitaeten_verschwinden() {
        assert_eq!(clean("<p>Text <b>fett</b></p>"), "Text fett");
        assert_eq!(clean("<![CDATA[Roh]]>"), "Roh");
        // Doppelt kodiert, wie bei Minecraft.net beobachtet.
        assert_eq!(clean("Capes &amp;amp; Promos"), "Capes & Promos");
    }

    #[test]
    fn lange_beschreibung_wird_gekuerzt() {
        let long = "Satz eins. ".repeat(60);
        let s = shorten(&long);
        assert!(s.chars().count() <= SUMMARY_LIMIT + 1);
        assert!(s.ends_with('…'), "gekürzt muss erkennbar sein");
    }

    #[test]
    fn kurze_beschreibung_bleibt_unangetastet() {
        assert_eq!(shorten("Kurz."), "Kurz.");
    }
}
