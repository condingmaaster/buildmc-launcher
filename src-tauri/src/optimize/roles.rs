//! Rollen statt fester Modlisten.
//!
//! Mods decken nicht dieselben Minecraft-Versionen ab: Bobby gibt es erst
//! ab 1.17, Indium wird ab Sodium 0.6 ueberfluessig, und eine Mod kann fuer
//! eine frische Version wochenlang fehlen. Eine feste Liste waere damit
//! bei der Haelfte aller Versionen kaputt.
//!
//! Deshalb wird beschrieben, *wozu* eine Mod da ist, und dazu mehrere
//! Anwaerter in absteigender Bevorzugung. Aufgeloest wird gegen Modrinth:
//! der erste Anwaerter, den es fuer diese Version gibt, gewinnt. Findet
//! sich keiner, faellt die Rolle aus -- mit Begruendung statt stillem
//! Weglassen.

use super::Level;

pub struct Role {
    pub id: &'static str,
    pub label: &'static str,
    /// Warum diese Rolle etwas bringt -- wird in der Oberflaeche gezeigt.
    pub why: &'static str,
    /// Modrinth-Kennungen, bevorzugte zuerst.
    pub candidates: &'static [&'static str],
    /// Ab welcher Stufe die Rolle dazugehoert.
    pub from_level: Level,
    /// Faellt die Rolle aus, ist das hinnehmbar?
    pub optional: bool,
}

/// Die Reihenfolge ist die Reihenfolge der Wirkung.
pub const ROLES: &[Role] = &[
    Role {
        id: "rendering",
        label: "Rendering-Engine",
        why: "Ersetzt Minecrafts Renderer. Der größte Einzelgewinn überhaupt.",
        candidates: &["sodium"],
        from_level: Level::Balanced,
        optional: false,
    },
    Role {
        id: "tick",
        label: "Spiellogik",
        why: "Optimiert die Tick-Schleife, ohne das Spielverhalten zu ändern.",
        candidates: &["lithium"],
        from_level: Level::Balanced,
        optional: true,
    },
    Role {
        id: "memory",
        label: "Speicherverbrauch",
        why: "Senkt den Speicherbedarf der Blockzustände deutlich.",
        // ModernFix als Ausweichlösung: greift breiter, aber flacher.
        candidates: &["ferrite-core", "modernfix"],
        from_level: Level::Balanced,
        optional: true,
    },
    Role {
        id: "render_api",
        label: "Rendering-Verträglichkeit",
        why: "Nötig, damit andere Mods mit Sodium zusammenarbeiten. \
              Ab Sodium 0.6 überflüssig und dann zu Recht nicht vorhanden.",
        candidates: &["indium"],
        from_level: Level::Balanced,
        optional: true,
    },
    Role {
        id: "render_extras",
        label: "Partikel und Animationen",
        why: "Erlaubt, Partikel und Animationen einzeln abzuschalten — \
              nach der Sichtweite der zweitgrößte Hebel.",
        candidates: &["sodium-extra"],
        from_level: Level::Performance,
        optional: true,
    },
    Role {
        id: "sodium_ui",
        label: "Einstellungsmenü",
        why: "Macht Sodiums Einstellungen im Spiel zugänglich.",
        candidates: &["reeses-sodium-options"],
        from_level: Level::Performance,
        optional: true,
    },
    Role {
        id: "immediate",
        label: "Oberflächen-Zeichnen",
        why: "Beschleunigt Schrift, Inventar und Anzeigen.",
        candidates: &["immediatelyfast"],
        from_level: Level::Performance,
        optional: true,
    },
    Role {
        id: "entity_culling",
        label: "Verdeckte Wesen",
        why: "Überspringt das Zeichnen von Wesen, die man ohnehin nicht sieht.",
        candidates: &["entityculling"],
        from_level: Level::Extreme,
        optional: true,
    },
    Role {
        id: "distant_terrain",
        label: "Weitsicht",
        why: "Speichert entferntes Gelände zwischen und zeichnet es weiter. \
              Erst dadurch ist Sichtweite 3 erträglich statt einer Nebelwand.",
        // Distant Horizons als Ausweichlösung: gleiches Ziel, anderer Weg,
        // deutlich aufwendiger -- deshalb zweite Wahl.
        candidates: &["bobby", "distanthorizons"],
        from_level: Level::Extreme,
        optional: false,
    },
];

/// Die Rollen einer Stufe.
pub fn for_level(level: Level) -> Vec<&'static Role> {
    ROLES.iter().filter(|r| level >= r.from_level).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stufen_bauen_aufeinander_auf() {
        let b = for_level(Level::Balanced).len();
        let p = for_level(Level::Performance).len();
        let e = for_level(Level::Extreme).len();
        assert!(b < p && p < e, "jede Stufe ergänzt die vorige");
    }

    #[test]
    fn rendering_ist_ueberall_dabei_und_unverzichtbar() {
        for level in [Level::Balanced, Level::Performance, Level::Extreme] {
            let roles = for_level(level);
            let rendering = roles.iter().find(|r| r.id == "rendering");
            assert!(rendering.is_some(), "{level:?}");
            assert!(!rendering.unwrap().optional);
        }
    }

    #[test]
    fn weitsicht_nur_bei_maximal_und_mit_ausweichloesung() {
        assert!(!for_level(Level::Performance).iter().any(|r| r.id == "distant_terrain"));
        let role = for_level(Level::Extreme)
            .into_iter().find(|r| r.id == "distant_terrain").unwrap();
        assert!(role.candidates.len() > 1, "braucht eine Alternative");
        assert_eq!(role.candidates[0], "bobby", "bevorzugt bleibt bobby");
    }

    #[test]
    fn kennungen_sind_kleingeschrieben_und_ohne_leerzeichen() {
        for role in ROLES {
            for c in role.candidates {
                assert!(!c.is_empty());
                assert_eq!(*c, &c.to_lowercase(), "{c}");
                assert!(!c.contains(' '), "{c}");
            }
        }
    }
}
