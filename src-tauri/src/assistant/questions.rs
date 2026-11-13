//! Der Fragenkatalog.
//!
//! Dieser feste Katalog trägt drei Aufgaben zugleich:
//!
//! * Er ist der Assistent **ohne** API-Schlüssel -- vollwertig, nur ohne
//!   Begründungen in eigenen Worten.
//! * Er ist der **Rückfall**, wenn das Modell mitten im Ablauf ausfällt.
//! * Er ist die **Vorlage**, an der sich das Modell orientiert, wenn es
//!   einen eigenen Katalog auf das Ziel zuschneidet.
//!
//! Die Antworten tragen ihre Wirkung selbst: jede trägt Suchbegriffe und
//! Hinweise zur Leistungsstufe. Dadurch braucht die Auswertung kein
//! Sprachmodell -- sie ist eine Zuordnung, keine Deutung.

use serde::{Deserialize, Serialize};

use crate::optimize::{Level, Pacing};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub prompt: String,
    /// Kurzer Hinweis, warum das gefragt wird.
    pub why: String,
    pub multi: bool,
    pub options: Vec<Option_>,
}

/// `Option` ist in Rust belegt, deshalb mit Unterstrich.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Option_ {
    pub id: String,
    pub label: String,
    pub detail: String,
    /// Modrinth-Kategorien. Wo es eine gibt, ist sie dem Volltext
    /// vorzuziehen: „optimization" als Kategorie findet Sodium und
    /// Lithium, als Suchwort nur Mods mit dem Wort im Namen.
    #[serde(default)]
    pub categories: Vec<String>,
    /// Volltext, wenn keine Kategorie passt -- englisch, weil die Portale
    /// englisch indexieren.
    #[serde(default)]
    pub queries: Vec<String>,
    /// Inhaltstyp der Suche.
    #[serde(default)]
    pub project_type: std::option::Option<String>,
}

fn opt(id: &str, label: &str, detail: &str, queries: &[&str]) -> Option_ {
    Option_ {
        id: id.into(),
        label: label.into(),
        detail: detail.into(),
        categories: Vec::new(),
        queries: queries.iter().map(|s| s.to_string()).collect(),
        project_type: None,
    }
}

/// Wie `opt`, aber mit Modrinth-Kategorien statt Volltext.
fn opt_cat(id: &str, label: &str, detail: &str, categories: &[&str]) -> Option_ {
    Option_ {
        categories: categories.iter().map(|s| s.to_string()).collect(),
        ..opt(id, label, detail, &[])
    }
}

/// Kategorien **und** Inhaltstyp -- fuer Shader und Resourcepacks, die
/// eigene Kategoriebaeume haben.
fn opt_cat_typed(id: &str, label: &str, detail: &str, categories: &[&str], kind: &str) -> Option_ {
    Option_ {
        categories: categories.iter().map(|s| s.to_string()).collect(),
        project_type: Some(kind.into()),
        ..opt(id, label, detail, &[])
    }
}

/// Was der Katalog über das System wissen muss.
///
/// Version, Loader und Arbeitsspeicher lassen sich nicht festschreiben:
/// die einen hängen am Manifest, der andere am Rechner. Ein fest
/// eingetragener Wert wäre bei der Hälfte aller Nutzer falsch.
#[derive(Debug, Clone)]
pub struct Context {
    /// Neueste Release-Versionen, neueste zuerst.
    pub versions: Vec<String>,
    /// Gesamter Systemspeicher in MB, 0 wenn unbekannt.
    pub system_ram_mb: u32,
}

impl Default for Context {
    fn default() -> Self {
        Self { versions: Vec::new(), system_ram_mb: 0 }
    }
}

/// Sinnvolle Vorschläge für den Arbeitsspeicher.
///
/// Mehr als die Hälfte des Systemspeichers bringt selten etwas und nimmt
/// dem Betriebssystem den Zwischenspeicher für Dateien weg -- das macht
/// das Spiel am Ende langsamer, nicht schneller.
pub fn ram_choices(system_ram_mb: u32) -> Vec<(u32, &'static str)> {
    let half = system_ram_mb / 2;
    let mut out: Vec<(u32, &'static str)> = Vec::new();

    for (mb, label) in [
        (2048u32, "2 GB — Vanilla und wenige Mods"),
        (4096, "4 GB — die übliche Wahl"),
        (6144, "6 GB — viele Mods oder große Sichtweite"),
        (8192, "8 GB — große Modpakete"),
        (12288, "12 GB — sehr große Pakete"),
    ] {
        // Nur anbieten, was der Rechner verträgt.
        if system_ram_mb == 0 || mb <= half.max(2048) {
            out.push((mb, label));
        }
    }
    out
}

/// Der Katalog. Reihenfolge ist Absicht: erst wohin, dann womit, dann wie viel.
pub fn catalogue(ctx: &Context) -> Vec<Question> {
    let mut out = vec![
        Question {
            id: "version".into(),
            prompt: "Welche Minecraft-Version?".into(),
            why: "Bestimmt, welche Mods es überhaupt gibt.".into(),
            multi: false,
            options: if ctx.versions.is_empty() {
                vec![opt("unbekannt", "Wird nachgeladen",
                         "Die Versionsliste konnte nicht abgerufen werden.", &[])]
            } else {
                ctx.versions.iter().take(8).enumerate().map(|(i, v)| {
                    opt(v, v, if i == 0 { "Neueste Ausgabe" } else { "" }, &[])
                }).collect()
            },
        },
        Question {
            id: "loader".into(),
            prompt: "Welcher Mod-Loader?".into(),
            why: "Fabric und Quilt lassen sich hier installieren; Mods sind zwischen Loadern nicht austauschbar.".into(),
            multi: false,
            options: vec![
                opt("fabric", "Fabric",
                    "Die größte Auswahl an Mods, schlank und schnell aktuell.", &[]),
                opt("quilt", "Quilt",
                    "Abspaltung von Fabric, verträgt sich mit den meisten Fabric-Mods.", &[]),
                opt("vanilla", "Keiner",
                    "Reines Minecraft ohne Mods. Dann entfallen die Modfragen.", &[]),
            ],
        },
    ];
    out.extend(vec![
        Question {
            id: "ziel".into(),
            prompt: "Worum soll es in dieser Instanz gehen?".into(),
            why: "Die wichtigste Frage -- sie bestimmt den Großteil der Suche. \
                  Mehreres ist möglich; Technik und Abenteuer schließen sich \
                  nicht aus. Wer alles ankreuzt, bekommt allerdings von jedem \
                  etwas und von nichts genug.".into(),
            multi: true,
            options: vec![
                opt_cat("vanilla_plus", "Vanilla, nur besser",
                    "Das Spielgefühl bleibt unangetastet. Es wird nur schneller \
                     und bequemer — der sicherste Einstieg.",
                    &["optimization", "utility"]),
                opt_cat("technik", "Technik und Automatisierung",
                    "Maschinen, Fließbänder, Energienetze. Wird schnell umfangreich \
                     und braucht mehr Arbeitsspeicher als die anderen Richtungen.",
                    &["technology", "transportation"]),
                opt_cat("abenteuer", "Abenteuer und Erkundung",
                    "Neue Biome, Strukturen und Gegner, für die sich das Loslaufen lohnt.",
                    &["adventure", "worldgen"]),
                opt_cat("bauen", "Bauen und Gestalten",
                    "Mehr Blöcke, Formen, Farben und Werkzeuge zum Setzen.",
                    &["decoration"]),
                opt_cat("magie", "Magie",
                    "Zauber, Rituale, Verzauberungen. Oft mit eigenem Fortschrittspfad.",
                    &["magic"]),
                opt_cat("landwirtschaft", "Anbau und Kochen",
                    "Mehr Pflanzen, Tiere und Gerichte. Verträgt sich gut mit allem anderen.",
                    &["food"]),
                opt_cat("kreaturen", "Mehr Kreaturen",
                    "Zusätzliche Tiere und Gegner, zahm wie feindlich.",
                    &["mobs"]),
                opt_cat("ausruestung", "Waffen und Rüstung",
                    "Neue Werkzeuge, Waffen, Rüstungssätze und Verzauberungen.",
                    &["equipment"]),
                opt_cat("regeln", "Andere Spielregeln",
                    "Greift in die Mechanik ein — Hunger, Schlaf, Sterben, Fortschritt.",
                    &["game-mechanics"]),
                opt_cat("handel", "Wirtschaft und Handel",
                    "Läden, Währungen, Handel mit Dorfbewohnern oder Mitspielern.",
                    &["economy"]),
                opt_cat("zusammen", "Miteinander spielen",
                    "Chat, Gilden, gemeinsame Aufgaben. Sinnvoll vor allem auf Servern.",
                    &["social"]),
                opt_cat("minispiele", "Minispiele",
                    "Abgeschlossene Spielmodi innerhalb der Welt.",
                    &["minigame"]),
            ],
        },
        Question {
            id: "spielstil".into(),
            prompt: "Wie spielst du am liebsten?".into(),
            why: "Bestimmt, ob Mods eher Arbeit abnehmen oder Widerstand \
                  hinzufügen sollen. Beides gleichzeitig ergibt selten ein \
                  stimmiges Paket.".into(),
            multi: false,
            options: vec![
                opt("gemuetlich", "Gemütlich",
                    "Aufbauen, sammeln, gestalten. Kämpfe stören eher.", &[]),
                opt("ausgewogen", "Ausgewogen",
                    "Ein bisschen von allem, so wie Minecraft gedacht ist.", &[]),
                opt("fordernd", "Fordernd",
                    "Das Spiel darf wehtun. Stärkere Gegner, knappere Mittel.", &[]),
                opt("aufbau", "Reines Aufbauen",
                    "Kreativ oder quasi-kreativ. Überleben ist Nebensache.", &[]),
            ],
        },
        Question {
            id: "welt".into(),
            prompt: "Soll die Welt anders aussehen?".into(),
            why: "Weltgenerierung wirkt nur auf **neu erzeugtes** Gelände. \
                  In einer bestehenden Welt bleibt alles Erkundete, wie es ist.".into(),
            multi: true,
            options: vec![
                opt_cat("biome", "Mehr Biome",
                    "Zusätzliche Landschaften mit eigener Vegetation.", &["worldgen"]),
                opt("strukturen", "Mehr Bauwerke",
                    "Dörfer, Ruinen, Verliese, die es zu finden gibt.",
                    &["structures", "dungeons"]),
                opt("dimensionen", "Neue Dimensionen",
                    "Eigene Welten neben Ober-, Nether- und Endwelt. Kostet am meisten \
                     Arbeitsspeicher, weil jede Dimension eigene Blöcke mitbringt.",
                    &["dimension"]),
                opt("hoehlen", "Andere Höhlen",
                    "Größere, tiefere oder belebtere Höhlensysteme.",
                    &["caves", "cave generation"]),
                opt("keine_welt", "Welt so lassen",
                    "Die Vanilla-Weltgenerierung bleibt unverändert.", &[]),
            ],
        },
        Question {
            id: "geraet".into(),
            prompt: "Wie stark ist dein Rechner?".into(),
            why: "Entscheidet über Sichtweite, Partikel und darüber, wie \
                  aggressiv optimiert wird.".into(),
            multi: false,
            options: vec![
                opt("schwach", "Eher schwach",
                    "Ältere oder eingebaute Grafik, wenig Arbeitsspeicher. \
                     Jedes Bild pro Sekunde zählt.", &[]),
                opt("mittel", "Durchschnittlich",
                    "Vanilla läuft flüssig, bei vielen Mods wird es eng.", &[]),
                opt("stark", "Kräftig",
                    "Genug Reserven. Optik darf etwas kosten.", &[]),
                opt("unbekannt", "Weiß ich nicht",
                    "Dann wird vorsichtig geschätzt — lieber etwas zu sparsam \
                     als ruckelnd.", &[]),
            ],
        },
        Question {
            id: "leistungsziel".into(),
            prompt: "Was ist dir bei der Leistung am wichtigsten?".into(),
            why: "Eine hohe Zahl und ein ruhiges Bild sind nicht dasselbe. \
                  Ein gedeckeltes, gleichmäßiges Bild wirkt oft flüssiger als \
                  eine höhere, aber schwankende Bildrate.".into(),
            multi: false,
            options: vec![
                opt("gleichmaessig", "Ein ruhiges Bild",
                    "Gedeckelt und gleichmäßig, kein Mikroruckeln. Für die meisten \
                     die bessere Wahl.", &[]),
                opt("hoechste", "Die höchste Bildrate",
                    "Kein Deckel. Größte Zahl, dafür schwankende Bildzeiten. \
                     Sinnvoll vor allem im Wettkampf.", &[]),
                opt("sichtweite", "Große Sichtweite",
                    "Weit sehen ist wichtiger als die Zahl. Braucht spürbar \
                     mehr Arbeitsspeicher.", &[]),
            ],
        },
        Question {
            id: "optik".into(),
            prompt: "Soll die Optik verändert werden?".into(),
            why: "Shader sind der mit Abstand teuerste Posten — sie können die \
                  Bildrate halbieren. Texturen kosten vor allem Arbeitsspeicher.".into(),
            multi: true,
            options: vec![
                opt_cat_typed("shader", "Shader",
                    "Licht, Schatten, Spiegelungen im Wasser. Sieht am besten aus \
                     und kostet am meisten.",
                    &["semi-realistic", "shadows"], "shader"),
                opt_cat_typed("shader_sparsam", "Sparsame Shader",
                    "Nur besseres Licht, ohne die teuren Spiegelungen. Läuft auch \
                     auf schwacher Grafik.",
                    &["low", "vanilla-like"], "shader"),
                opt_cat_typed("texturen_treu", "Texturen, Vanilla-nah",
                    "Schärfer, aber erkennbar Minecraft. Der verträglichste Weg.",
                    &["vanilla-like", "16x"], "resourcepack"),
                opt_cat_typed("texturen_hoch", "Texturen, hochauflösend",
                    "Deutlich detaillierter. Braucht Arbeitsspeicher und passt \
                     schlecht zu schwacher Grafik.",
                    &["realistic", "64x"], "resourcepack"),
                opt_cat_typed("oberflaeche", "Andere Menüs und Anzeigen",
                    "Aufgeräumtere Bedienoberfläche, lesbarere Schrift.",
                    &["gui", "fonts"], "resourcepack"),
                opt("keine", "Nichts davon",
                    "Vanilla-Optik behalten. Die schnellste Wahl.", &[]),
            ],
        },
        Question {
            id: "audio".into(),
            prompt: "Soll sich am Klang etwas ändern?".into(),
            why: "Kostet praktisch keine Leistung und wird trotzdem fast immer \
                  vergessen.".into(),
            multi: true,
            options: vec![
                opt("raumklang", "Räumlicher Klang",
                    "Hall in Höhlen, gedämpfter Klang hinter Wänden.",
                    &["sound physics", "audio extension"]),
                opt_cat_typed("klangpaket", "Anderes Klangpaket",
                    "Ersetzt Spielgeräusche und Musik.", &["audio"], "resourcepack"),
                opt("keine_audio", "Klang so lassen", "", &[]),
            ],
        },
        Question {
            id: "komfort".into(),
            prompt: "Welche Bequemlichkeiten willst du?".into(),
            why: "Kleine Mods, die den Alltag angenehmer machen. Sie kosten \
                  kaum Leistung und sind das, was man nach einer Woche am \
                  wenigsten missen will.".into(),
            multi: true,
            options: vec![
                opt("karte", "Minikarte und Wegpunkte",
                    "Orientierung ohne ständiges F3. Auf manchen Servern \
                     eingeschränkt.", &["minimap"]),
                opt("rezepte", "Rezepte nachschlagen",
                    "Zeigt zu jedem Gegenstand, wie er hergestellt wird — die \
                     erste Mod, die man bei vielen Mods braucht.",
                    &["recipe viewer"]),
                opt_cat("inventar", "Inventar sortieren",
                    "Sortieren und Umschichten mit weniger Klicks.", &["management"]),
                opt_cat("lager", "Besser lagern",
                    "Größere Kisten, Fässer, durchsuchbare Lager.", &["storage"]),
                opt("zoom", "Heranzoomen",
                    "Fernglas-Taste wie in Optifine.", &["zoom"]),
                opt("hinweise", "Mehr Hinweise im Spiel",
                    "Sprechblasen, Werkzeuginfos, Haltbarkeitsanzeigen.",
                    &["tooltips", "hud"]),
                opt("keine_extras", "Nichts davon",
                    "Nur das, was die anderen Antworten ergeben.", &[]),
            ],
        },
        Question {
            id: "barrierefreiheit".into(),
            prompt: "Brauchst du Erleichterungen bei der Bedienung?".into(),
            why: "Wird selten gefragt und hilft vielen. Nichts davon kostet \
                  Leistung.".into(),
            multi: true,
            options: vec![
                opt("farbsehen", "Farbsehschwäche berücksichtigen",
                    "Kräftigere, unterscheidbare Farben für Anzeigen und Tränke.",
                    &["colorblind"]),
                opt("lesbarkeit", "Größere, klarere Schrift",
                    "Bessere Lesbarkeit in Menüs und im Chat.", &["font", "readable"]),
                opt("untertitel", "Untertitel und sichtbare Geräusche",
                    "Zeigt an, was zu hören ist und aus welcher Richtung.",
                    &["subtitles", "accessibility"]),
                opt("keine_hilfen", "Nichts davon", "", &[]),
            ],
        },
        Question {
            id: "server".into(),
            prompt: "Willst du auf fremden Servern spielen?".into(),
            why: "Manche Mod-Arten gelten dort als unerlaubt. Geprüft werden \
                  kann das nicht — es gibt keine Liste, was welcher Server duldet.".into(),
            multi: false,
            options: vec![
                opt("ja", "Ja, auch auf öffentlichen Servern",
                    "Du bekommst einen Hinweis, welche Mod-Arten dort häufig \
                     unerwünscht sind.", &[]),
                opt("freunde", "Nur auf einem eigenen Server mit Freunden",
                    "Dort gelten eure Regeln. Wichtig ist nur, dass alle dieselben \
                     Mods haben.", &[]),
                opt("nein", "Nur allein",
                    "Keine Einschränkung.", &[]),
            ],
        },
        Question {
            id: "umfang".into(),
            prompt: "Wie groß soll das Paket werden?".into(),
            why: "Mehr Mods heißt längere Startzeit, mehr Arbeitsspeicher und \
                  mehr Gelegenheiten für Unverträglichkeiten.".into(),
            multi: false,
            options: vec![
                opt("schlank", "Schlank",
                    "Eine Handvoll Mods, die sicher zusammenpassen. Startet schnell.", &[]),
                opt("mittel", "Mittel",
                    "Ein rundes Paket für das gewählte Ziel.", &[]),
                opt("gross", "Groß",
                    "Viel Inhalt. Rechne mit längeren Startzeiten und gelegentlichem \
                     Nachbessern.", &[]),
            ],
        },
    ]);

    out.push(Question {
        id: "ram".into(),
        prompt: "Wie viel Arbeitsspeicher soll die Instanz bekommen?".into(),
        why: if ctx.system_ram_mb > 0 {
            format!(
                "Dein Rechner hat {} GB. Mehr als die Hälfte zu geben bringt \
                 selten etwas — dem System fehlt dann der Dateizwischenspeicher.",
                ctx.system_ram_mb / 1024
            )
        } else {
            "Mehr als die Hälfte des Systemspeichers bringt selten etwas.".into()
        },
        multi: false,
        options: ram_choices(ctx.system_ram_mb)
            .into_iter()
            .map(|(mb, label)| opt(&mb.to_string(), label, "", &[]))
            .collect(),
    });

    out
}

/// Was aus den Antworten folgt -- ohne jede Deutung.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Outcome {
    /// Je Eintrag: Inhaltstyp, Suchwort, Kategorien.
    pub searches: Vec<(String, String, Vec<String>)>,
    pub level: std::option::Option<Level>,
    pub pacing: std::option::Option<Pacing>,
    pub mc_version: std::option::Option<String>,
    pub loader: std::option::Option<String>,
    pub max_ram: std::option::Option<u32>,
    /// Hinweise, die in der Oberfläche erscheinen.
    pub notes: Vec<String>,
}

/// Leitet aus den Antworten Suchen und Leistungsstufe ab.
///
/// Rein mechanisch: gleiche Antworten ergeben immer dasselbe. Genau das
/// erlaubt den Betrieb ohne Sprachmodell.
/// Wertet gegen den eingebauten Katalog aus.
pub fn evaluate(answers: &[(String, Vec<String>)]) -> Outcome {
    evaluate_in(&catalogue(&Context::default()), answers)
}

/// Wertet gegen **die tatsaechlich gestellten** Fragen aus.
///
/// Der Assistent darf seinen Fragebogen vom Modell zuschneiden lassen.
/// Dann heissen die Fragen anders als im Katalog -- statt `ziel` etwa
/// `performance_mods`. Wer hier stur im Katalog nachschlaegt, findet
/// keinen einzigen Suchbegriff und die Auswahl bleibt leer, ohne dass
/// irgendwo ein Fehler auftaucht.
pub fn evaluate_in(questions: &[Question], answers: &[(String, Vec<String>)]) -> Outcome {
    let mut out = Outcome::default();
    let picked = |frage: &str, wahl: &str| {
        answers.iter().any(|(q, opts)| q == frage && opts.iter().any(|o| o == wahl))
    };

    let single = |frage: &str| -> std::option::Option<String> {
        answers.iter().find(|(q, _)| q == frage).and_then(|(_, o)| o.first().cloned())
    };

    out.mc_version = single("version").filter(|v| v != "unbekannt");
    out.loader = single("loader");
    out.max_ram = single("ram").and_then(|v| v.parse().ok());

    for (question_id, chosen) in answers {
        let Some(question) = questions.iter().find(|q| &q.id == question_id) else { continue };
        for choice in chosen {
            let Some(option) = question.options.iter().find(|o| &o.id == choice) else { continue };
            let kind = option.project_type.clone().unwrap_or_else(|| "mod".into());

            // Kategorien in einer Suche zusammen -- sie sind eine ODER-Gruppe.
            if !option.categories.is_empty() {
                out.searches.push((kind.clone(), String::new(), option.categories.clone()));
            }
            for query in &option.queries {
                out.searches.push((kind.clone(), query.clone(), Vec::new()));
            }
        }
    }

    // Harte Garantie, unabhaengig davon, woher die Fragen kamen: eine
    // leere Suche darf es nie geben. Der bisherige Rueckfall griff nur,
    // wenn das Modell *fremde* Frage-IDs erfand -- er half nicht, wenn es
    // stattdessen bekannte IDs wiederverwendete, deren Optionen hier
    // schlicht keine Suchbegriffe tragen (genau das ist einmal passiert:
    // "leistungsziel" und "optik: keine" tragen beide keine, und das
    // Modell hatte die einzige Frage mit garantiertem Suchbegriff --
    // "ziel" -- gar nicht erst gestellt). Diese Pruefung steht deshalb
    // absichtlich *nach* jedem anderen Zuschnitt und ist an keine
    // Bedingung mehr geknuepft ausser der einen, die zaehlt: ist die Liste
    // leer?
    if out.searches.is_empty() {
        out.searches.push((
            "mod".into(),
            String::new(),
            vec!["optimization".into(), "utility".into()],
        ));
        out.notes.push(
            "Zu diesen Antworten allein gab es keinen eigenen Suchansatz. \
             Vorgeschlagen werden deshalb verbreitete, verträgliche \
             Leistungs-Mods als sichere Grundlage — das ist keine auf dich \
             zugeschnittene Wahl, sondern die Absicherung dagegen, dass \
             am Ende gar nichts vorgeschlagen wird."
                .into(),
        );
    }

    // Leistungsstufe aus Gerät, Optik und ausdrücklichem Leistungsziel.
    let shader = picked("optik", "shader") || picked("optik", "shader_sparsam");
    let teure_optik = picked("optik", "shader") || picked("optik", "texturen_hoch");
    // "Weiß ich nicht" wird wie schwach behandelt -- lieber sparsam als ruckelnd.
    let schwach = picked("geraet", "schwach") || picked("geraet", "unbekannt");

    out.level = Some(if schwach || picked("leistungsziel", "hoechste") {
        Level::Extreme
    } else if picked("geraet", "stark") && !teure_optik {
        Level::Balanced
    } else {
        Level::Performance
    });

    // Das Leistungsziel entscheidet über den Bilddeckel.
    out.pacing = Some(if picked("leistungsziel", "hoechste") {
        Pacing::Unlimited
    } else {
        Pacing::default()
    });

    /* ---- Hinweise ----------------------------------------------------- */

    if teure_optik && schwach {
        out.notes.push(
            "Shader oder hochauflösende Texturen auf schwacher Grafik kosten viel. \
             Die Optimierung steht deshalb auf Maximal — falls es trotzdem hakt, \
             sind sie der erste Hebel."
                .into(),
        );
    }
    if shader && !picked("loader", "fabric") && !picked("loader", "quilt") {
        out.notes.push(
            "Shader brauchen Iris, und Iris gibt es nur für Fabric und Quilt. \
             Ohne einen der beiden Lader bleibt es bei der Vanilla-Beleuchtung."
                .into(),
        );
    }
    if picked("server", "ja") {
        out.notes.push(
            "Für öffentliche Server: Minikarten mit Höhlenansicht und alles, was \
             Gegner durch Wände zeigt, gilt vielerorts als unerlaubt."
                .into(),
        );
    }
    if picked("server", "freunde") {
        out.notes.push(
            "Auf einem eigenen Server müssen alle dieselben Mods in derselben \
             Fassung haben — sonst kommt niemand hinein."
                .into(),
        );
    }
    if picked("welt", "dimensionen") {
        out.notes.push(
            "Neue Dimensionen wirken nur in einer neu erstellten Welt und brauchen \
             mehr Arbeitsspeicher als die übrigen Mods."
                .into(),
        );
    }
    if picked("welt", "biome") || picked("welt", "strukturen") || picked("welt", "hoehlen") {
        out.notes.push(
            "Weltgenerierung greift nur in noch nicht erkundetem Gelände. In einer \
             bestehenden Welt siehst du das Neue erst weit draußen."
                .into(),
        );
    }
    if picked("umfang", "gross") {
        out.notes.push(
            "Bei einem großen Paket ist der erste Start langsam und einzelne Mods \
             vertragen sich erfahrungsgemäß nicht auf Anhieb. Die Diagnose im \
             Instanzfenster benennt den Verursacher."
                .into(),
        );
    }
    if picked("leistungsziel", "sichtweite") {
        out.notes.push(
            "Große Sichtweite ist vor allem eine Frage des Arbeitsspeichers. Wenn es \
             ruckelt, ist die Sichtweite der erste Regler, nicht die Mods."
                .into(),
        );
    }
    if picked("spielstil", "aufbau") {
        out.notes.push(
            "Zum reinen Bauen lohnt sich der Kreativmodus — viele Bau-Mods entfalten \
             ihren Nutzen erst mit unbegrenztem Material."
                .into(),
        );
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ans(pairs: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        pairs.iter()
            .map(|(q, o)| (q.to_string(), o.iter().map(|s| s.to_string()).collect()))
            .collect()
    }

    fn ctx() -> Context {
        Context {
            versions: vec!["1.21.4".into(), "1.21.1".into(), "1.20.1".into()],
            system_ram_mb: 16384,
        }
    }

    #[test]
    fn katalog_ist_in_sich_stimmig() {
        for q in catalogue(&ctx()) {
            assert!(!q.options.is_empty(), "{} ohne Auswahl", q.id);
            assert!(!q.why.is_empty(), "{} ohne Begründung", q.id);
            let mut ids: Vec<&str> = q.options.iter().map(|o| o.id.as_str()).collect();
            ids.sort();
            let vorher = ids.len();
            ids.dedup();
            assert_eq!(ids.len(), vorher, "{} hat doppelte Kennungen", q.id);
        }
    }

    #[test]
    fn schwaches_geraet_bekommt_die_hoechste_stufe() {
        let o = evaluate(&ans(&[("geraet", &["schwach"])]));
        assert_eq!(o.level, Some(Level::Extreme));
    }

    #[test]
    fn starkes_geraet_ohne_shader_bleibt_ausgewogen() {
        let o = evaluate(&ans(&[("geraet", &["stark"]), ("optik", &["keine"])]));
        assert_eq!(o.level, Some(Level::Balanced));
    }

    #[test]
    fn shader_auf_starkem_geraet_ziehen_die_stufe_an() {
        let o = evaluate(&ans(&[("geraet", &["stark"]), ("optik", &["shader"])]));
        assert_eq!(o.level, Some(Level::Performance));
    }

    #[test]
    fn suchbegriffe_kommen_aus_den_antworten() {
        let o = evaluate(&ans(&[("ziel", &["technik"]), ("komfort", &["karte"])]));
        // Technik läuft über die Kategorie, Minikarte über den Volltext.
        assert!(o.searches.iter().any(|(_, _, c)| c.iter().any(|x| x == "technology")));
        assert!(o.searches.iter().any(|(_, q, _)| q == "minimap"));
    }

    #[test]
    fn shader_werden_als_shader_gesucht_nicht_als_mod() {
        let o = evaluate(&ans(&[("optik", &["shader"])]));
        assert!(o.searches.iter().any(|(kind, _, _)| kind == "shader"));
    }

    #[test]
    fn warnung_bei_shadern_auf_schwachem_geraet() {
        let o = evaluate(&ans(&[("geraet", &["schwach"]), ("optik", &["shader"])]));
        assert!(o.notes.iter().any(|n| n.contains("Shader")));
    }

    #[test]
    fn gleiche_antworten_ergeben_gleiches_ergebnis() {
        let a = evaluate(&ans(&[("ziel", &["abenteuer"]), ("geraet", &["mittel"])]));
        let b = evaluate(&ans(&[("ziel", &["abenteuer"]), ("geraet", &["mittel"])]));
        assert_eq!(a.searches, b.searches);
        assert_eq!(a.level, b.level);
    }

    #[test]
    fn unbekannte_antworten_stoeren_nicht() {
        // Frueher: ergab eine leere Suche. Seit der Garantie greift hier
        // stattdessen das Sicherheitsnetz -- leer bleibt es nie mehr.
        let o = evaluate(&ans(&[("gibt_es_nicht", &["egal"])]));
        assert!(!o.searches.is_empty(), "das Sicherheitsnetz muss hier greifen");
        assert!(o.level.is_some(), "die Stufe steht trotzdem");
    }

    /// Regression: das Modell darf den Fragebogen umbauen -- eigene Fragen,
    /// eigene Kennungen, eigene Auswahltexte. `evaluate` schlug bisher
    /// gegen den festen Katalog nach und fand dort nichts, weil die
    /// Kennungen nicht uebereinstimmten. Die Suche blieb leer, ohne dass
    /// irgendwo ein Fehler auftauchte -- der Assistent haengte einfach auf
    /// "Vorschläge werden gesucht".
    #[test]
    fn fragebogen_vom_modell_liefert_trotzdem_suchbegriffe() {
        let fragen = vec![Question {
            id: "performance_mods".into(),
            prompt: "Sollen Leistungs-Mods rein?".into(),
            why: "Testfrage vom Modell.".into(),
            multi: false,
            options: vec![
                opt("ja", "Ja", "", &["sodium", "iris", "starlight"]),
                opt("nein", "Nein", "", &[]),
            ],
        }];
        let antworten = ans(&[("performance_mods", &["ja"])]);
        let o = evaluate_in(&fragen, &antworten);
        assert!(!o.searches.is_empty(), "eigene Fragen duerfen nicht leer suchen");
        assert!(o.searches.iter().any(|(_, q, _)| q == "sodium"));
    }

    /// Traegt der zugeschnittene Fragebogen selbst schon Suchbegriffe --
    /// weil das Modell sie mitgeliefert hat --, darf der Katalog-Rueckfall
    /// nicht zusaetzlich hineinfunken und die Auswahl verdoppeln.
    #[test]
    fn eigene_suchbegriffe_werden_nicht_verdoppelt() {
        let fragen = vec![Question {
            id: "mein_ziel".into(),
            prompt: "Testfrage".into(),
            why: "".into(),
            multi: false,
            options: vec![opt("a", "A", "", &["nur_dieses_wort"])],
        }];
        let o = evaluate_in(&fragen, &ans(&[("mein_ziel", &["a"])]));
        let treffer: Vec<_> = o.searches.iter().filter(|(_, q, _)| q == "nur_dieses_wort").collect();
        assert_eq!(treffer.len(), 1, "{:?}", o.searches);
    }

    /// Der echte Vorfall: das Modell hat "ziel" durch "leistungsziel"
    /// ersetzt (eine rein technische Frage ohne jeden Suchbegriff) und
    /// jede gewaehlte Option -- ueberall -- trug leere queries/categories.
    /// Der alte Rueckfall griff nicht, weil alle Frage-IDs zufaellig
    /// bekannt waren. Das darf nie wieder in einer leeren Suche enden.
    #[test]
    fn garantiert_nie_leere_suche_bei_reiner_opt_out_antwort() {
        let zugeschnitten = vec![
            Question {
                id: "geraet".into(), prompt: "".into(), why: "".into(), multi: false,
                options: vec![opt("schwach", "Schwach", "", &[])],
            },
            Question {
                id: "leistungsziel".into(), prompt: "".into(), why: "".into(), multi: false,
                options: vec![opt("gleichmaessig", "Gleichmäßig", "", &[])],
            },
            Question {
                id: "optik".into(), prompt: "".into(), why: "".into(), multi: true,
                options: vec![opt("keine", "Keine", "", &[])],
            },
        ];
        let antworten = ans(&[
            ("geraet", &["schwach"]),
            ("leistungsziel", &["gleichmaessig"]),
            ("optik", &["keine"]),
            ("loader", &["fabric"]),
            ("ram", &["2048"]),
            ("version", &["1.21.10"]),
        ]);

        let o = evaluate_in(&zugeschnitten, &antworten);
        assert!(!o.searches.is_empty(), "genau dieser Fall lief leer, bevor die Garantie kam");
        assert!(o.notes.iter().any(|n| n.contains("sichere Grundlage")));
    }

    /// Auch der Standardkatalog selbst darf nicht leerlaufen, wenn jede
    /// Frage mit ihrer neutralsten/"nichts davon"-Option beantwortet wird.
    #[test]
    fn standardkatalog_mit_ueberall_nichts_davon_bleibt_nicht_leer() {
        let antworten = ans(&[
            ("version", &["1.21.4"]),
            ("loader", &["vanilla"]),
            ("ziel", &[]),
            ("spielstil", &["ausgewogen"]),
            ("welt", &["keine_welt"]),
            ("geraet", &["mittel"]),
            ("leistungsziel", &["gleichmaessig"]),
            ("optik", &["keine"]),
            ("audio", &["keine_audio"]),
            ("komfort", &["keine_extras"]),
            ("barrierefreiheit", &["keine_hilfen"]),
            ("server", &["nein"]),
            ("umfang", &["schlank"]),
            ("ram", &["2048"]),
        ]);
        let o = evaluate(&antworten);
        assert!(!o.searches.is_empty(), "der Standardkatalog selbst lief leer");
    }
}


#[cfg(test)]
mod katalog_tests {
    use super::*;

    fn ctx(ram: u32) -> Context {
        Context { versions: vec!["1.21.4".into(), "1.21.1".into()], system_ram_mb: ram }
    }

    #[test]
    fn version_und_loader_stehen_vorn() {
        let k = catalogue(&ctx(16384));
        assert_eq!(k[0].id, "version");
        assert_eq!(k[1].id, "loader");
        // Die neueste Version ist die erste Wahl.
        assert_eq!(k[0].options[0].label, "1.21.4");
    }

    #[test]
    fn ohne_versionsliste_bleibt_die_frage_bedienbar() {
        let k = catalogue(&Context::default());
        assert!(!k[0].options.is_empty(), "eine leere Auswahl wäre eine Sackgasse");
    }

    #[test]
    fn ziel_ist_mehrfach_waehlbar() {
        let k = catalogue(&ctx(16384));
        let ziel = k.iter().find(|q| q.id == "ziel").unwrap();
        assert!(ziel.multi, "Technik und Abenteuer schließen sich nicht aus");
    }

    #[test]
    fn arbeitsspeicher_richtet_sich_nach_dem_rechner() {
        // 8 GB System -> hoechstens 4 GB anbieten.
        let klein = ram_choices(8192);
        assert!(klein.iter().all(|(mb, _)| *mb <= 4096), "{klein:?}");

        // 32 GB System -> auch die grossen Stufen.
        let gross = ram_choices(32768);
        assert!(gross.iter().any(|(mb, _)| *mb == 12288));
    }

    #[test]
    fn sehr_wenig_speicher_ergibt_trotzdem_eine_wahl() {
        // Auch bei 2 GB muss etwas anwaehlbar sein, sonst haengt der Ablauf.
        assert!(!ram_choices(2048).is_empty());
    }

    #[test]
    fn unbekannter_systemspeicher_bietet_alles_an() {
        assert_eq!(ram_choices(0).len(), 5);
    }

    #[test]
    fn version_loader_und_ram_kommen_im_ergebnis_an() {
        let antworten = vec![
            ("version".to_string(), vec!["1.21.4".to_string()]),
            ("loader".to_string(), vec!["fabric".to_string()]),
            ("ram".to_string(), vec!["6144".to_string()]),
        ];
        let o = evaluate(&antworten);
        assert_eq!(o.mc_version.as_deref(), Some("1.21.4"));
        assert_eq!(o.loader.as_deref(), Some("fabric"));
        assert_eq!(o.max_ram, Some(6144));
    }

    #[test]
    fn platzhalter_gilt_nicht_als_version() {
        let antworten = vec![("version".to_string(), vec!["unbekannt".to_string()])];
        assert!(evaluate(&antworten).mc_version.is_none());
    }
}
