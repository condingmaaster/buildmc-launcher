//! Optimierungsstufen.
//!
//! Eine frische Instanz auf Bildrate trimmen, ohne ein Dutzend Menues
//! durchzuklicken. Die Werte stammen aus einer real getunten Installation
//! und sind nach Wirkung gestaffelt.
//!
//! Zwei Achsen, die bewusst getrennt bleiben:
//!
//! * **Detailstufe** (`Level`) -- wie viel gezeichnet wird.
//! * **Bildlauf** (`Pacing`) -- ob die Bildrate gedeckelt wird.
//!
//! Die Trennung ist wichtig, weil beides verschiedene Ziele verfolgt. Eine
//! ungedeckelte Bildrate schwankt mit der Szene, und schwankende Bildzeiten
//! nimmt man als Mikroruckeln wahr. Deckelt man unterhalb dessen, was die
//! Hardware schafft, hat jedes Bild Luft und die Abstaende werden
//! gleichmaessig -- ruhige 90 wirken besser als zappelnde 200. Wer dagegen
//! die hoechste Zahl will, schaltet den Deckel ab.
//!
//! **Vorhandene Werte werden nicht ueberschrieben**, sofern der Nutzer sie
//! selbst gesetzt hat: die Datei wird zusammengefuehrt, nicht ersetzt.

pub mod roles;
pub mod resolve;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;

/// Die Reihenfolge der Varianten traegt Bedeutung: eine hoehere Stufe
/// enthaelt alles aus den niedrigeren. Darauf stuetzt sich `for_level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    /// Sichtbar unveraendert, nur die teuren Kleinigkeiten aus.
    Balanced,
    /// Deutlich schneller, Optik merklich schlichter.
    Performance,
    /// Alles heruntergedreht, was Bilder kostet. Sieht karg aus.
    Extreme,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Balanced => "Ausgewogen",
            Level::Performance => "Leistung",
            Level::Extreme => "Maximal",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Level::Balanced =>
                "Partikel und Animationen sparsam, Sichtweite 8. Sieht fast \
                 unverändert aus und bringt schon spürbar mehr.",
            Level::Performance =>
                "Sichtweite 6, keine Wolken, keine Entity-Schatten, \
                 Umgebungsverdeckung aus. Sichtbar schlichter, deutlich schneller.",
            Level::Extreme =>
                "Sichtweite 3, alle Partikel und Animationen aus, kein Mipmapping. \
                 Sieht karg aus — Bobby täuscht die Weite vor, damit es erträglich bleibt.",
        }
    }
}

/// Wie die Bildrate gefuehrt wird.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pacing {
    /// Gedeckelt und auf den Monitor synchronisiert. Gleichmaessige
    /// Bildzeiten, kein Mikroruckeln -- die ruhigere Wahl.
    Smooth { cap: u32 },
    /// Kein Deckel. Hoechste Zahl, dafuer schwankende Bildzeiten.
    Unlimited,
}

impl Default for Pacing {
    /// Gleichmaessigkeit ist fuer die meisten die bessere Voreinstellung:
    /// sie wirkt fluessiger, auch wenn die Zahl kleiner ist.
    fn default() -> Self {
        Pacing::Smooth { cap: 90 }
    }
}

impl Pacing {
    pub fn label(self) -> &'static str {
        match self {
            Pacing::Smooth { .. } => "Gleichmäßig",
            Pacing::Unlimited => "Höchste Bildrate",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Pacing::Smooth { .. } =>
                "Bildrate gedeckelt und auf den Monitor synchronisiert. Jedes Bild \
                 hat Luft, die Abstände bleiben gleich — dadurch kein Mikroruckeln.",
            Pacing::Unlimited =>
                "Kein Deckel. Zeigt die höchstmögliche Zahl, aber die Bildzeiten \
                 schwanken mit der Szene.",
        }
    }

    fn apply(self, o: &mut BTreeMap<&'static str, String>) {
        match self {
            Pacing::Smooth { cap } => {
                o.insert("enableVsync", "true".into());
                o.insert("maxFps", cap.clamp(30, 260).to_string());
            }
            Pacing::Unlimited => {
                o.insert("enableVsync", "false".into());
                // 260 ist in Minecraft die Stufe "unbegrenzt".
                o.insert("maxFps", "260".into());
            }
        }
    }
}

/// Mods, die je Stufe installiert werden.
///
/// Angegeben als Modrinth-Kennungen; die passende Fassung sucht der
/// Launcher zur Minecraft-Version, statt Versionsnummern festzuschreiben.
pub fn mod_set(level: Level) -> &'static [&'static str] {
    match level {
        // Die drei wirken ohne jede sichtbare Aenderung.
        Level::Balanced => &["sodium", "lithium", "ferritecore"],
        Level::Performance => &[
            "sodium", "lithium", "ferritecore", "sodium-extra", "reeses-sodium-options",
        ],
        // Bobby ist hier kein Beiwerk: er macht Sichtweite 3 ertraeglich,
        // indem er entferntes Gelaende zwischenspeichert und weiterzeichnet.
        Level::Extreme => &[
            "sodium", "lithium", "ferritecore", "sodium-extra",
            "reeses-sodium-options", "bobby", "entityculling",
        ],
    }
}

/* ---- options.txt ------------------------------------------------------- */

/// Die Werte fuer `options.txt`. Minecraft schreibt `schluessel:wert`.
pub fn options(level: Level, pacing: Pacing) -> BTreeMap<&'static str, String> {
    let mut o: BTreeMap<&'static str, String> = BTreeMap::new();

    pacing.apply(&mut o);

    let (render, simulation) = match level {
        Level::Balanced => (8, 8),
        Level::Performance => (6, 6),
        Level::Extreme => (3, 5),
    };
    o.insert("renderDistance", render.to_string());
    o.insert("simulationDistance", simulation.to_string());

    // 0 = alle, 1 = vermindert, 2 = minimal
    o.insert("particles", match level {
        Level::Balanced => "1".into(),
        _ => "2".into(),
    });

    o.insert("graphicsMode", match level {
        Level::Balanced => "1".into(),   // schoen
        _ => "0".into(),                 // schnell
    });

    if level != Level::Balanced {
        o.insert("ao", "false".into());
        o.insert("biomeBlendRadius", "0".into());
        o.insert("entityShadows", "false".into());
        o.insert("renderClouds", "\"false\"".into());
        o.insert("cloudRange", "2".into());
        o.insert("entityDistanceScaling", "0.75".into());
        o.insert("cutoutLeaves", "false".into());
    }

    if level == Level::Extreme {
        o.insert("mipmapLevels", "0".into());
        o.insert("entityDistanceScaling", "0.5".into());
        o.insert("screenEffectScale", "0.0".into());
        o.insert("darknessEffectScale", "0.0".into());
        o.insert("glintSpeed", "0.0".into());
        o.insert("glintStrength", "0.0".into());
        o.insert("bobView", "false".into());
        o.insert("maxAnisotropyBit", "0".into());
    }

    o
}

/// Schreibt die Werte in eine vorhandene `options.txt`, ohne sie zu ersetzen.
///
/// Minecraft speichert dort auch Tastenbelegungen, Lautstaerke und
/// Sprachwahl. Die Datei zu ueberschreiben wuerde das alles verwerfen.
pub fn merge_options(path: &std::path::Path, level: Level, pacing: Pacing) -> Result<usize> {
    let existing = std::fs::read_to_string(path).unwrap_or_default();
    let wanted = options(level, pacing);

    let mut lines: Vec<String> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();

    for line in existing.lines() {
        match line.split_once(':') {
            Some((key, _)) if wanted.contains_key(key) => {
                let key_owned = wanted.get_key_value(key).unwrap().0;
                seen.push(key_owned);
                lines.push(format!("{key}:{}", wanted[key]));
            }
            _ => lines.push(line.to_string()),
        }
    }

    // Was noch nicht in der Datei stand, hinten anhaengen.
    for (key, value) in &wanted {
        if !seen.contains(key) {
            lines.push(format!("{key}:{value}"));
        }
    }

    crate::paths::ensure_parent(path)?;
    std::fs::write(path, lines.join("\n") + "\n")?;
    Ok(wanted.len())
}

/* ---- Sodium ------------------------------------------------------------ */

pub fn sodium_options(level: Level) -> serde_json::Value {
    let aggressive = level != Level::Balanced;
    serde_json::json!({
        "quality": {
            "hidden_fluid_culling": true,
            "improved_fluid_shaping": !aggressive,
            "use_closest_point_entity_sort": false,
            "pixel_filtering_mode": if level == Level::Extreme { "NEAREST" } else { "LINEAR" },
        },
        "performance": {
            // 0 bedeutet: so viele Faeden wie Kerne.
            "chunk_builder_threads": 0,
            "chunk_build_defer_mode": if aggressive { "ALWAYS" } else { "ONE_FRAME" },
            "animate_only_visible_textures": true,
            "use_entity_culling": true,
            "use_fog_occlusion": true,
            "use_block_face_culling": true,
            // Spart die GL-Fehlerpruefung je Aufruf.
            "use_no_error_g_l_context": aggressive,
            "quad_splitting_mode": "SAFE",
        },
        "advanced": {
            "enable_memory_tracing": false,
            "use_advanced_staging_buffers": true,
            "cpu_render_ahead_limit": 9,
        },
    })
}

/// Sodium Extra: hier liegen Partikel und Animationen.
///
/// Sie einzeln abzuschalten bringt bei vollen Szenen mehr als jede andere
/// Einzelmassnahme ausser der Sichtweite.
pub fn sodium_extra_options(level: Level) -> serde_json::Value {
    let animations = level == Level::Balanced;
    let particles = level == Level::Balanced;

    serde_json::json!({
        "animation_settings": {
            "animation": animations,
            "water": animations,
            "lava": animations,
            "fire": animations,
            "portal": animations,
            "block_animations": animations,
            "sculk_sensor": animations,
        },
        "particle_settings": {
            "particles": particles,
            "rain_splash": particles,
            "block_break": particles,
            "block_breaking": particles,
        },
        "detail_settings": {
            "sky": level != Level::Extreme,
            "stars": level != Level::Extreme,
            "sun_moon": true,
            "weather": level != Level::Extreme,
            "biome_colors": level == Level::Balanced,
            "cloud_height": 128,
        },
        "render_settings": {
            "use_fast_random": true,
            "linear_flat_color_blender": false,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gleichmaessigkeit ist die Voreinstellung -- ein Deckel unterhalb
    /// dessen, was die Hardware schafft, verhindert Mikroruckeln.
    #[test]
    fn gleichmaessig_deckelt_und_synchronisiert() {
        for level in [Level::Balanced, Level::Performance, Level::Extreme] {
            let o = options(level, Pacing::default());
            assert_eq!(o["enableVsync"], "true", "{level:?}");
            assert_eq!(o["maxFps"], "90", "{level:?}");
        }
    }

    #[test]
    fn hoechste_bildrate_nimmt_den_deckel_weg() {
        for level in [Level::Balanced, Level::Performance, Level::Extreme] {
            let o = options(level, Pacing::Unlimited);
            assert_eq!(o["enableVsync"], "false", "{level:?}");
            assert_eq!(o["maxFps"], "260", "{level:?}");
        }
    }

    #[test]
    fn deckel_bleibt_im_sinnvollen_bereich() {
        let o = options(Level::Balanced, Pacing::Smooth { cap: 5 });
        assert_eq!(o["maxFps"], "30", "zu kleiner Wert wird angehoben");
        let o = options(Level::Balanced, Pacing::Smooth { cap: 9999 });
        assert_eq!(o["maxFps"], "260", "zu großer Wert wird gekappt");
    }

    #[test]
    fn stufen_sind_tatsaechlich_gestaffelt() {
        let r = |l| options(l, Pacing::default())["renderDistance"].parse::<u32>().unwrap();
        assert!(r(Level::Balanced) > r(Level::Performance));
        assert!(r(Level::Performance) > r(Level::Extreme));
    }

    #[test]
    fn ausgewogen_bleibt_ansehnlich() {
        let o = options(Level::Balanced, Pacing::default());
        // Umgebungsverdeckung und Wolken bleiben an.
        assert!(!o.contains_key("ao"));
        assert!(!o.contains_key("renderClouds"));
        assert_eq!(o["graphicsMode"], "1");
    }

    #[test]
    fn bobby_nur_bei_maximal() {
        assert!(mod_set(Level::Extreme).contains(&"bobby"));
        assert!(!mod_set(Level::Balanced).contains(&"bobby"));
        // Die schmerzfreien drei sind ueberall dabei.
        for level in [Level::Balanced, Level::Performance, Level::Extreme] {
            for m in ["sodium", "lithium", "ferritecore"] {
                assert!(mod_set(level).contains(&m), "{m} fehlt bei {level:?}");
            }
        }
    }

    #[test]
    fn options_werden_zusammengefuehrt_nicht_ersetzt() {
        let dir = std::env::temp_dir().join("buildmc-opt-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("options.txt");
        // Eigene Werte, die erhalten bleiben muessen.
        std::fs::write(&file, "lang:de_de\nkey_key.attack:key.mouse.left\nrenderDistance:16\n").unwrap();

        merge_options(&file, Level::Extreme, Pacing::default()).unwrap();
        let after = std::fs::read_to_string(&file).unwrap();

        assert!(after.contains("lang:de_de"), "Sprache darf nicht verloren gehen");
        assert!(after.contains("key_key.attack:key.mouse.left"), "Tastenbelegung bleibt");
        assert!(after.contains("renderDistance:3"), "Wert wird ersetzt");
        assert!(!after.contains("renderDistance:16"), "alter Wert ist weg");

        std::fs::remove_file(&file).ok();
    }

    #[test]
    fn maximal_schaltet_partikel_und_animationen_ab() {
        let e = sodium_extra_options(Level::Extreme);
        assert_eq!(e["animation_settings"]["water"], false);
        assert_eq!(e["particle_settings"]["particles"], false);
        // Ausgewogen laesst sie an.
        let b = sodium_extra_options(Level::Balanced);
        assert_eq!(b["animation_settings"]["water"], true);
    }
}
