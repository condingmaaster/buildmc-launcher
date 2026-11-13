//! Protokollauswertung.
//!
//! Ziel ist nicht, irgendeinen Fehler zu finden, sondern den *ausloesenden*.
//! Das ist in Minecraft-Protokollen selten die erste rote Zeile: ein Fehler
//! beim Laden einer Mod reisst reihenweise Folgefehler nach sich, und der
//! eigentliche Grund steht am Ende einer `Caused by:`-Kette.
//!
//! Der Ablauf ist deshalb dreistufig:
//!
//! 1. Das Protokoll wird in Zeilen mit Dringlichkeit zerlegt.
//! 2. Aus dem Fehlerbereich wird die Ausnahmekette gezogen und ihre
//!    *tiefste* Ursache bestimmt -- sie benennt das eigentliche Problem.
//! 3. Eine Regeltafel ordnet dieser Ursache eine benannte Diagnose zu.
//!    Gewinner ist die Regel mit der hoechsten Trennschaerfe, nicht die
//!    erste, die zufaellig passt.
//!
//! Der Verursacher wird getrennt davon aus den Stapelrahmen und den
//! Meldungen des Mod-Laders bestimmt.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnosis {
    /// Die benannte Ursache -- kurz, in der Sprache des Nutzers.
    pub cause: String,
    /// Warum es dazu kam.
    pub explanation: String,
    /// Was konkret zu tun ist, in sinnvoller Reihenfolge.
    pub suggestions: Vec<String>,
    /// Vermutlich verantwortliche Mod, sofern zuzuordnen.
    pub culprit_mod: Option<String>,
    /// Die Zeile, auf die sich alles stuetzt.
    pub trigger_line: String,
    /// 0..100. Unter 40 wird die Diagnose als Vermutung ausgewiesen.
    pub confidence: u8,
}

/* ---- Zerlegung -------------------------------------------------------- */

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone)]
pub struct Line {
    pub level: Level,
    pub message: String,
    pub raw: String,
}

/// Liest die Dringlichkeit aus `[hh:mm:ss] [Thread/LEVEL]: Text`.
/// Faellt auf die Textsuche zurueck, wenn das Format abweicht -- Fabric,
/// Forge und die Startskripte schreiben nicht alle gleich.
pub fn parse(text: &str) -> Vec<Line> {
    let mut out = Vec::with_capacity(text.lines().count());
    let mut getragen = Level::Info;

    for raw in text.lines() {
        // Fortsetzungszeilen -- Stapelrahmen, `Caused by:`, der Wortlaut
        // einer Ausnahme, Fabrics Loesungsvorschlaege -- tragen keine
        // eigene Angabe. Sie gehoeren zur Zeile darueber. Ohne das faellt
        // genau die Zeile, die die Ausnahme benennt, auf Info zurueck und
        // die Ursachenkette bleibt leer.
        let level = match own_level(raw) {
            Some(l) => l,
            None if is_continuation(raw) => getragen,
            None => Level::Info,
        };
        getragen = level;

        // Alles nach dem letzten "]: " ist die eigentliche Meldung.
        let message = match raw.find("]: ") {
            Some(at) => raw[at + 3..].to_string(),
            None => raw.trim().to_string(),
        };

        out.push(Line { level, message, raw: raw.to_string() });
    }

    out
}

/// Die Dringlichkeit, die eine Zeile selbst angibt -- `None`, wenn keine.
fn own_level(raw: &str) -> Option<Level> {
    if raw.contains("/FATAL]") || raw.contains("/ERROR]") || raw.contains("[ERROR]") {
        return Some(Level::Error);
    }
    if raw.contains("/WARN]") || raw.contains("[WARN]") {
        return Some(Level::Warn);
    }
    if raw.starts_with("Exception") {
        return Some(Level::Error);
    }
    if raw.contains("/INFO]") || raw.contains("[INFO]") || raw.contains("/DEBUG]") {
        return Some(Level::Info);
    }
    None
}

/// Eine Zeile ohne fuehrenden Zeitstempel setzt die vorige fort.
fn is_continuation(raw: &str) -> bool {
    !raw.trim().is_empty() && !raw.starts_with('[')
}

/* ---- Ursachenkette ---------------------------------------------------- */

/// Ein Glied der Kette: voll qualifizierter Ausnahmename und Meldung.
#[derive(Debug, Clone, PartialEq)]
pub struct Cause {
    pub class: String,
    pub detail: String,
    pub raw: String,
}

/// Erkennt `paket.Klasse: Meldung` bzw. `paket.Klasse` am Zeilenanfang.
fn as_exception(message: &str) -> Option<Cause> {
    let trimmed = message.trim();
    if trimmed.starts_with("at ") || trimmed.is_empty() {
        return None;
    }

    let head = trimmed.strip_prefix("Caused by: ").unwrap_or(trimmed);
    let head = head.strip_prefix("Exception in thread ").unwrap_or(head);

    let (class, detail) = match head.split_once(": ") {
        Some((c, d)) => (c.trim(), d.trim()),
        None => (head.trim(), ""),
    };

    // Ein Ausnahmename traegt einen Punkt und endet nicht auf Satzzeichen.
    let plausible = class.contains('.')
        && !class.contains(' ')
        && class
            .rsplit('.')
            .next()
            .is_some_and(|s| s.chars().next().is_some_and(char::is_uppercase));

    if !plausible {
        return None;
    }

    Some(Cause {
        class: class.to_string(),
        detail: detail.to_string(),
        raw: trimmed.to_string(),
    })
}

/// Zieht alle Ausnahmen in Reihenfolge ihres Auftretens.
pub fn causes(lines: &[Line]) -> Vec<Cause> {
    lines
        .iter()
        .filter(|l| l.level == Level::Error)
        .filter_map(|l| as_exception(&l.message))
        .collect()
}

/// Die tiefste Ursache: das letzte Glied der Kette.
///
/// Java schreibt `Caused by:` von aussen nach innen, die innerste Ursache
/// steht also zuletzt. Genau die benennt den echten Grund.
pub fn root_cause(lines: &[Line]) -> Option<Cause> {
    let all = causes(lines);
    all.into_iter().last()
}

/* ---- Verursacher ------------------------------------------------------ */

/// Paketpraefixe, die nie ein Mod-Verursacher sind.
const NEUTRAL: [&str; 12] = [
    "java.",
    "javax.",
    "jdk.",
    "sun.",
    "net.minecraft.",
    "com.mojang.",
    "net.fabricmc.loader.",
    "org.spongepowered.",
    "cpw.mods.",
    "net.minecraftforge.",
    "net.neoforged.",
    "org.lwjgl.",
];

/// Bestimmt die verantwortliche Mod.
///
/// Die Meldungen des Mod-Laders sind zuverlaessiger als die Stapelrahmen
/// und werden deshalb zuerst befragt. Erst danach wird der erste Rahmen
/// gesucht, der nicht zu Minecraft, Java oder dem Loader gehoert.
pub fn culprit(lines: &[Line]) -> Option<String> {
    for line in lines.iter().filter(|l| l.level != Level::Info) {
        let m = &line.message;

        // Fabric: "Mixin apply for mod <id> failed"
        if let Some(rest) = m.strip_prefix("Mixin apply for mod ") {
            if let Some(id) = rest.split_whitespace().next() {
                return Some(id.to_string());
            }
        }

        // Fabric: "Mod 'Name' (id)" / "mod 'id'"
        if let Some(id) = between(m, "Mod '", "'").or_else(|| between(m, "mod '", "'")) {
            return Some(id);
        }

        // Forge/NeoForge: "Failure message: ... modid"
        if let Some(id) = between(m, "Mod ID: ", " ") {
            return Some(id);
        }
    }

    // Stapelrahmen
    for line in lines.iter() {
        let t = line.message.trim();
        let Some(frame) = t.strip_prefix("at ") else { continue };
        let Some(path) = frame.split('(').next() else { continue };
        if NEUTRAL.iter().any(|p| path.starts_with(p)) {
            continue;
        }
        if !path.contains('.') {
            continue;
        }
        // Zweites Paketsegment ist meistens der Projektname:
        // "tr7zw.entityculling.EntityCullingMod" -> "entityculling"
        let mut segs = path.split('.');
        let first = segs.next().unwrap_or("");
        let second = segs.next().unwrap_or("");
        let name = if second.is_empty() || second.chars().next().is_some_and(char::is_uppercase) {
            first
        } else {
            second
        };
        if !name.is_empty() {
            return Some(name.to_string());
        }
    }

    None
}

fn between(haystack: &str, open: &str, close: &str) -> Option<String> {
    let start = haystack.find(open)? + open.len();
    let rest = &haystack[start..];
    let end = rest.find(close)?;
    let found = rest[..end].trim();
    (!found.is_empty()).then(|| found.to_string())
}

/* ---- Regeltafel ------------------------------------------------------- */

/// Was einer Regel zur Verfuegung steht.
struct Evidence<'a> {
    root: Option<&'a Cause>,
    /// Das Protokoll in Originalschreibung -- fuer Namen, die die
    /// kleingeschriebene Fassung unbrauchbar macht.
    text: &'a str,
    /// Das gesamte Protokoll in Kleinschreibung, fuer Volltextpruefungen.
    haystack: String,
    culprit: Option<String>,
}

impl Evidence<'_> {
    fn class_is(&self, needle: &str) -> bool {
        self.root.is_some_and(|c| c.class.ends_with(needle))
    }
    fn detail_has(&self, needle: &str) -> bool {
        self.root
            .is_some_and(|c| c.detail.to_ascii_lowercase().contains(&needle.to_ascii_lowercase()))
    }
    fn log_has(&self, needle: &str) -> bool {
        self.haystack.contains(&needle.to_ascii_lowercase())
    }
    fn trigger(&self) -> String {
        self.root.map(|c| c.raw.clone()).unwrap_or_default()
    }
    /// Die erste Zeile, die `needle` enthaelt -- in Originalschreibung.
    fn line_with(&self, needle: &str) -> Option<&str> {
        let lower = needle.to_ascii_lowercase();
        self.text
            .lines()
            .map(str::trim)
            .find(|l| l.to_ascii_lowercase().contains(&lower))
    }
    fn mod_name(&self) -> String {
        self.culprit.clone().unwrap_or_else(|| "die betroffene Mod".into())
    }
}

struct Rule {
    /// Je hoeher, desto trennschaerfer. Gewinner ist der hoechste Wert,
    /// damit eine spezifische Regel eine allgemeine nie verliert.
    specificity: u8,
    confidence: u8,
    test: fn(&Evidence) -> bool,
    build: fn(&Evidence) -> (String, String, Vec<String>),
}

/// Zieht die beiden Mod-Namen aus Fabrics Unvertraeglichkeitszeile:
/// `Mod 'Sodium' (sodium) ... incompatible with ... mod 'VulkanMod' (vulkanmod)`.
fn quoted_pair(line: &str) -> Option<(String, String)> {
    let mut namen = line.split('\'').skip(1).step_by(2).map(str::trim);
    let a = namen.next()?.to_string();
    let b = namen.next()?.to_string();
    (!a.is_empty() && !b.is_empty()).then_some((a, b))
}

fn v(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

const RULES: &[Rule] = &[
    /* -- Java ---------------------------------------------------------- */
    Rule {
        specificity: 95,
        confidence: 97,
        test: |e| e.class_is("UnsupportedClassVersionError"),
        build: |e| (
            "Java ist zu alt für diese Minecraft-Version".into(),
            format!(
                "Die Klassendateien wurden für eine neuere Java-Fassung übersetzt, \
                 als die gerade verwendete lesen kann. Minecraft ab 1.17 braucht \
                 mindestens Java 16, ab 1.18 Java 17 und ab 1.20.5 Java 21.\n\n{}",
                e.trigger()
            ),
            v(&[
                "Unter Einstellungen → Spiel ein neueres Java auswählen",
                "Alternativ ein passendes Java über den Launcher installieren lassen",
                "Das Feld für den Java-Pfad leeren, damit automatisch gesucht wird",
            ]),
        ),
    },
    /* -- Arbeitsspeicher ----------------------------------------------- */
    Rule {
        specificity: 94,
        confidence: 95,
        test: |e| e.class_is("OutOfMemoryError") && e.detail_has("heap space"),
        build: |_| (
            "Der zugewiesene Arbeitsspeicher reicht nicht".into(),
            "Die Java-Halde ist vollgelaufen. Das passiert bei vielen Mods, \
             großer Sichtweite oder hochauflösenden Resourcepacks. Mehr \
             Speicher zuzuweisen hilft — allerdings nur bis etwa zur Hälfte \
             des vorhandenen Systemspeichers, darüber hinaus wird es eher \
             langsamer.".into(),
            v(&[
                "Den Höchstwert für den Arbeitsspeicher der Instanz anheben",
                "Sichtweite und Auflösung der Resourcepacks verringern",
                "Speicherhungrige Mods wie Distant Horizons vorübergehend abschalten",
            ]),
        ),
    },
    Rule {
        specificity: 94,
        confidence: 92,
        test: |e| e.class_is("OutOfMemoryError") && e.detail_has("metaspace"),
        build: |_| (
            "Der Klassenspeicher ist vollgelaufen".into(),
            "Nicht die Halde, sondern der Metaspace ist erschöpft — dort legt \
             Java die geladenen Klassen ab. Bei sehr vielen Mods reicht die \
             Voreinstellung mitunter nicht.".into(),
            v(&[
                "Den JVM-Schalter -XX:MaxMetaspaceSize=512m ergänzen",
                "Die Zahl der installierten Mods verringern",
            ]),
        ),
    },
    /* -- Fehlende Abhaengigkeit ---------------------------------------- */
    Rule {
        specificity: 92,
        confidence: 94,
        test: |e| e.log_has("requires") && (e.log_has("which is missing") || e.log_has("but none was found")),
        build: |_| (
            "Eine benötigte Begleit-Mod fehlt".into(),
            "Eine installierte Mod setzt eine andere voraus, die nicht \
             vorhanden ist. Der Mod-Lader bricht deshalb schon vor dem \
             Spielstart ab. Im Protokoll steht, welche Mod welche vermisst."
                .into(),
            v(&[
                "Die vermisste Mod unter „Entdecken“ suchen und installieren",
                "Beim Installieren den Abhängigkeitsplan bestätigen statt zu überspringen",
                "Prüfen, ob die vermisste Mod nur abgeschaltet ist",
            ]),
        ),
    },
    /* -- Zwei Mods vertragen sich nicht --------------------------------- */
    Rule {
        specificity: 93,
        confidence: 95,
        test: |e| {
            e.log_has("is incompatible with any version of mod")
                || e.log_has("neg_hard_dep")
                || (e.log_has("incompatible mods found") && e.log_has("each other"))
        },
        build: |e| {
            let paar = e
                .line_with("is incompatible with any version of mod")
                .and_then(quoted_pair);

            let kopf = match &paar {
                Some((a, b)) => format!("{a} und {b} schließen sich gegenseitig aus"),
                None => "Zwei Mods schließen sich gegenseitig aus".to_string(),
            };

            let erklaerung = match &paar {
                Some((a, b)) => format!(
                    "{a} erklärt {b} ausdrücklich für unverträglich — beide greifen \
                     an derselben Stelle ins Spiel ein, meistens beim Rendern. Der \
                     Mod-Lader bricht deshalb schon vor dem Spielstart ab, statt einen \
                     Absturz mitten im Spiel zu riskieren. Eine von beiden muss weichen; \
                     eine neuere Fassung hilft nur, wenn die Unverträglichkeit dort \
                     aufgehoben wurde."
                ),
                None => "Zwei installierte Mods erklären sich gegenseitig für \
                         unverträglich. Der Mod-Lader bricht deshalb schon vor dem \
                         Spielstart ab."
                    .to_string(),
            };

            // Fabric rechnet selbst aus, was helfen wuerde -- das ist
            // belastbarer als jede Vermutung unsererseits.
            let mut schritte = Vec::new();
            if let Some(fix) = e.line_with("- Replace mod ") {
                schritte.push(fix.trim_start_matches("- ").to_string());
            }
            match &paar {
                Some((a, b)) => {
                    schritte.push(format!("Unter „Mods“ entweder {a} oder {b} abschalten"));
                    schritte.push(format!(
                        "Prüfen, ob es von {a} eine Fassung gibt, die {b} zulässt"
                    ));
                }
                None => schritte.push("Unter „Mods“ eine der beiden Mods abschalten".into()),
            }
            schritte.push("Nach dem Abschalten erneut starten — der Lader nennt \
                           sonst das nächste Paar".into());

            (kopf, format!("{erklaerung}\n\n{}", e.trigger()), schritte)
        },
    },
    /* -- Auflösung allgemein gescheitert -------------------------------- */
    Rule {
        specificity: 88,
        confidence: 88,
        test: |e| e.log_has("mod resolution failed") || e.log_has("incompatible mods found"),
        build: |e| (
            "Der Mod-Lader konnte die Mod-Liste nicht auflösen".into(),
            format!(
                "Fabric prüft vor dem Start, ob alle Mods zueinander und zur \
                 Minecraft-Version passen. Diese Prüfung ist fehlgeschlagen, das \
                 Spiel wurde deshalb gar nicht erst gestartet. Der Lader nennt den \
                 Grund in der Zeile „Reason“ und schlägt darunter selbst eine \
                 Lösung vor.\n\n{}",
                e.line_with("Reason:").unwrap_or(&e.trigger()),
            ),
            v(&[
                "Die im Protokoll genannte Mod aktualisieren oder abschalten",
                "Prüfen, ob alle Mods für dieselbe Minecraft-Version gebaut sind",
                "Zuletzt hinzugefügte Mods zuerst verdächtigen",
            ]),
        ),
    },
    /* -- Falsche Minecraft-Version ------------------------------------- */
    Rule {
        specificity: 90,
        confidence: 90,
        test: |e| {
            (e.class_is("NoSuchMethodError") || e.class_is("NoSuchFieldError"))
                && (e.detail_has("net.minecraft") || e.detail_has("class_"))
        },
        build: |e| (
            "Mod passt nicht zur Minecraft-Version".into(),
            format!(
                "{} greift auf etwas zu, das es in dieser Minecraft-Version \
                 nicht gibt. Die Mod wurde für eine andere Fassung gebaut. \
                 Der Lader startet sie trotzdem, der Fehler fällt erst beim \
                 Zugriff auf.\n\n{}",
                e.mod_name(),
                e.trigger()
            ),
            v(&[
                "Eine für genau diese Minecraft-Version gebaute Fassung installieren",
                "Die Mod abschalten und erneut starten, um sie als Ursache zu bestätigen",
                "Unter „Mods“ prüfen, ob weitere Einträge ein Update anbieten",
            ]),
        ),
    },
    /* -- Mixin ---------------------------------------------------------- */
    Rule {
        specificity: 88,
        confidence: 88,
        test: |e| {
            e.log_has("mixin apply for mod")
                || e.log_has("critical injection failure")
                || e.class_is("InjectionError")
        },
        build: |e| (
            "Ein Eingriff ins Spiel schlug fehl".into(),
            format!(
                "{} verändert den Spielcode über einen Mixin, findet die \
                 erwartete Stelle aber nicht. Das deutet fast immer darauf \
                 hin, dass Mod und Minecraft-Version nicht zusammenpassen — \
                 seltener darauf, dass zwei Mods dieselbe Stelle verändern.",
                e.mod_name()
            ),
            v(&[
                "Die genannte Mod auf eine passende Fassung bringen",
                "Sie abschalten und erneut starten",
                "Bei zwei konkurrierenden Mods eine von beiden entfernen",
            ]),
        ),
    },
    /* -- Falscher Loader ----------------------------------------------- */
    Rule {
        specificity: 86,
        confidence: 91,
        test: |e| {
            e.class_is("ClassNotFoundException")
                && (e.detail_has("net.minecraftforge") || e.detail_has("net.neoforged")
                    || e.detail_has("net.fabricmc"))
        },
        build: |_| (
            "Mod gehört zu einem anderen Mod-Lader".into(),
            "Es wurde eine Mod geladen, die für einen anderen Lader gebaut \
             ist — etwa eine Forge-Mod in einer Fabric-Instanz. Solche Mods \
             sind untereinander nicht austauschbar.".into(),
            v(&[
                "Die Mod entfernen und die Fassung für den Lader dieser Instanz holen",
                "Beim Herunterladen auf die Loader-Angabe achten",
                "Notfalls eine zweite Instanz mit dem passenden Lader anlegen",
            ]),
        ),
    },
    /* -- Doppelte Mod --------------------------------------------------- */
    Rule {
        specificity: 84,
        confidence: 93,
        test: |e| e.log_has("duplicate mod") || e.log_has("is present in multiple"),
        build: |_| (
            "Dieselbe Mod liegt doppelt vor".into(),
            "Zwei Dateien melden dieselbe Mod-Kennung. Das passiert, wenn \
             beim Aktualisieren die alte Datei liegen bleibt.".into(),
            v(&[
                "Unter „Mods“ den älteren der beiden Einträge entfernen",
                "Den Mods-Ordner auf Dateien mit dem Zusatz .old oder (1) durchsehen",
            ]),
        ),
    },
    /* -- Grafik --------------------------------------------------------- */
    Rule {
        specificity: 82,
        confidence: 86,
        test: |e| {
            e.log_has("pixel format not accelerated")
                || e.log_has("failed to create window")
                || e.log_has("no opengl context")
                || e.log_has("glfw error")
                || e.class_is("GLFWError")
        },
        build: |_| (
            "Die Grafikausgabe ließ sich nicht starten".into(),
            "Das Fenster oder der OpenGL-Kontext konnte nicht angelegt \
             werden. Ursache ist meist ein veralteter Grafiktreiber, eine \
             fehlende Hardwarebeschleunigung oder eine Fernsitzung ohne \
             Grafikzugriff.".into(),
            v(&[
                "Den Grafiktreiber aktualisieren",
                "Sicherstellen, dass die dedizierte Grafikkarte verwendet wird",
                "Shader und Resourcepacks entfernen und erneut starten",
            ]),
        ),
    },
    Rule {
        specificity: 80,
        confidence: 84,
        test: |e| e.class_is("UnsatisfiedLinkError") || e.log_has("no lwjgl in java.library.path"),
        build: |_| (
            "Native Bibliotheken fehlen".into(),
            "Minecraft findet die systemnahen Bibliotheken nicht, die es für \
             Fenster, Eingabe und Ton braucht. Meist sind sie unvollständig \
             entpackt worden.".into(),
            v(&[
                "Die Version im Launcher erneut herunterladen",
                "Prüfen, ob ein Virenschutz Dateien in Quarantäne genommen hat",
            ]),
        ),
    },
    /* -- Dateisystem ----------------------------------------------------- */
    Rule {
        specificity: 78,
        confidence: 95,
        test: |e| e.log_has("no space left on device") || e.detail_has("nicht genügend speicherplatz"),
        build: |_| (
            "Auf dem Datenträger ist kein Platz mehr".into(),
            "Das Schreiben schlug fehl, weil der Datenträger voll ist. \
             Minecraft legt während des Spielens laufend Daten an.".into(),
            v(&["Speicherplatz freigeben", "Alte Instanzen oder Sicherungen entfernen"]),
        ),
    },
    Rule {
        specificity: 76,
        confidence: 82,
        test: |e| {
            e.class_is("ZipException")
                || e.log_has("invalid cen header")
                || e.log_has("error in opening zip file")
        },
        build: |_| (
            "Eine Datei ist beschädigt".into(),
            "Ein Jar-Archiv ließ sich nicht öffnen. Der Download wurde \
             vermutlich abgebrochen oder die Datei wurde beim Schreiben \
             beschädigt.".into(),
            v(&[
                "Die Version erneut herunterladen",
                "Die zuletzt hinzugefügte Mod entfernen und neu installieren",
                "Unter Einstellungen die Prüfsummenkontrolle einschalten",
            ]),
        ),
    },
    Rule {
        specificity: 74,
        confidence: 80,
        test: |e| e.class_is("AccessDeniedException") || e.log_has("permission denied"),
        build: |_| (
            "Zugriff auf eine Datei verweigert".into(),
            "Das Betriebssystem hat den Zugriff abgelehnt. Meist hält ein \
             anderer Vorgang die Datei noch offen oder die Rechte stimmen \
             nicht.".into(),
            v(&[
                "Prüfen, ob die Instanz noch läuft, und sie beenden",
                "Den Launcher neu starten",
                "Die Rechte am Spielverzeichnis prüfen",
            ]),
        ),
    },
    /* -- Auffangregel ---------------------------------------------------- */
    Rule {
        specificity: 10,
        confidence: 35,
        test: |e| e.root.is_some(),
        build: |e| (
            format!(
                "Unbehandelter Fehler in {}",
                e.root
                    .and_then(|c| c.class.rsplit('.').next())
                    .unwrap_or("unbekannter Komponente")
            ),
            format!(
                "Die Ursache ließ sich keinem bekannten Fehlerbild zuordnen. \
                 Die auslösende Zeile lautet:\n\n{}",
                e.trigger()
            ),
            v(&[
                "Die zuletzt hinzugefügte Mod abschalten und erneut starten",
                "Die Instanz einmal ohne Mods starten, um sie als Ursache auszuschließen",
                "Den Protokollauszug bei der Mod melden",
            ]),
        ),
    },
];

/// Wertet ein Protokoll aus und benennt die wahrscheinlichste Ursache.
///
/// Gibt `None` zurueck, wenn nichts auf einen Fehlschlag hindeutet -- ein
/// sauber beendetes Spiel soll keine Diagnose erzeugen.
pub fn analyze(text: &str) -> Option<Diagnosis> {
    let lines = parse(text);
    if !lines.iter().any(|l| l.level == Level::Error) {
        return None;
    }

    let root = root_cause(&lines);
    let evidence = Evidence {
        root: root.as_ref(),
        text,
        haystack: text.to_ascii_lowercase(),
        culprit: culprit(&lines),
    };

    let rule = RULES
        .iter()
        .filter(|r| (r.test)(&evidence))
        .max_by_key(|r| r.specificity)?;

    let (cause, explanation, suggestions) = (rule.build)(&evidence);

    Some(Diagnosis {
        cause,
        explanation,
        suggestions,
        culprit_mod: evidence.culprit.clone(),
        trigger_line: evidence.trigger(),
        confidence: rule.confidence,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(body: &str) -> String {
        body.trim().to_string()
    }

    #[test]
    fn sauberes_protokoll_ergibt_keine_diagnose() {
        let t = log("
[21:40:58] [main/INFO]: Loading Minecraft 1.21.4
[21:41:07] [Render thread/INFO]: OpenAL initialized
[21:44:02] [main/INFO]: Stopping!
");
        assert!(analyze(&t).is_none());
    }

    #[test]
    fn warnungen_allein_sind_kein_absturz() {
        let t = log("
[21:41:04] [Render thread/WARN]: Unable to resolve texture reference
");
        assert!(analyze(&t).is_none());
    }

    /// Der Kern: die *tiefste* Ursache zaehlt, nicht die erste rote Zeile.
    #[test]
    fn tiefste_ursache_gewinnt() {
        let t = log("
[21:41:11] [main/ERROR]: Failed to start game
[21:41:11] [main/ERROR]: java.lang.RuntimeException: Mod loading failed
[21:41:11] [main/ERROR]: Caused by: java.lang.OutOfMemoryError: Java heap space
");
        let d = analyze(&t).expect("Diagnose erwartet");
        assert_eq!(d.cause, "Der zugewiesene Arbeitsspeicher reicht nicht");
    }

    #[test]
    fn java_zu_alt() {
        let t = log("
[main/ERROR]: java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main has been compiled by a more recent version of the Java Runtime
");
        let d = analyze(&t).unwrap();
        assert!(d.cause.contains("Java ist zu alt"));
        assert!(d.confidence > 90);
    }

    #[test]
    fn falsche_minecraft_version_mit_verursacher() {
        let t = log("
[Render thread/ERROR]: Mixin apply for mod entity-culling failed
[Render thread/ERROR]: java.lang.NoSuchMethodError: net.minecraft.class_310.method_1551()
[Render thread/ERROR]: \tat tr7zw.entityculling.EntityCullingMod.onInit(EntityCullingMod.java:84)
");
        let d = analyze(&t).unwrap();
        assert_eq!(d.cause, "Mod passt nicht zur Minecraft-Version");
        assert_eq!(d.culprit_mod.as_deref(), Some("entity-culling"));
    }

    #[test]
    fn fehlende_abhaengigkeit() {
        let t = log("
[main/ERROR]: Mod 'Cool Mod' (coolmod) 1.2.0 requires version 0.90.0 or later of fabric-api, which is missing!
");
        let d = analyze(&t).unwrap();
        assert!(d.cause.contains("Begleit-Mod fehlt"));
        assert_eq!(d.culprit_mod.as_deref(), Some("Cool Mod"));
    }

    #[test]
    fn falscher_loader() {
        let t = log("
[main/ERROR]: java.lang.ClassNotFoundException: net.minecraftforge.fml.common.Mod
");
        let d = analyze(&t).unwrap();
        assert!(d.cause.contains("anderen Mod-Lader"));
    }

    #[test]
    fn grafikfehler() {
        let t = log("
[main/ERROR]: GLFW error 65542: WGL: The driver does not appear to support OpenGL
[main/ERROR]: Failed to create window
");
        let d = analyze(&t).unwrap();
        assert!(d.cause.contains("Grafikausgabe"));
    }

    #[test]
    fn unbekanntes_faellt_auf_auffangregel() {
        let t = log("
[main/ERROR]: com.example.weird.VeryOwnError: etwas ganz Eigenes
");
        let d = analyze(&t).unwrap();
        assert!(d.cause.starts_with("Unbehandelter Fehler"));
        assert!(d.confidence < 40, "Vermutung muss als solche erkennbar sein");
    }

    #[test]
    fn stapelrahmen_ueberspringt_neutrale_pakete() {
        let lines = parse("
[main/ERROR]: java.lang.NullPointerException
[main/ERROR]: \tat java.base/java.util.Objects.requireNonNull(Objects.java:220)
[main/ERROR]: \tat net.minecraft.client.Minecraft.run(Minecraft.java:1)
[main/ERROR]: \tat com.terraformersmc.modmenu.ModMenu.onInit(ModMenu.java:44)
");
        assert_eq!(culprit(&lines).as_deref(), Some("terraformersmc"));
    }

    /// Das echte Protokoll aus der Instanz "Fabrictest": Fabric bricht
    /// wegen Sodium/VulkanMod ab. Die rote Zeile selbst ist keine
    /// Ausnahme -- die steht unpraefigiert darunter.
    #[test]
    fn fabric_unvertraeglichkeit_wird_erkannt() {
        let t = log("
[22:31:25] [main/INFO]: Loading Minecraft 1.21.4 with Fabric Loader 0.19.5
[22:31:26] [main/WARN]: Mod resolution failed
[22:31:26] [main/INFO]: Reason: [NEG_HARD_DEP sodium 0.6.13+mc1.21.4 {breaks vulkanmod @ [*]}]
[22:31:26] [main/ERROR]: Incompatible mods found!
net.fabricmc.loader.impl.FormattedException: Some of your mods are incompatible with the game or each other!
A potential solution has been determined, this may resolve your problem:
\t- Replace mod 'Sodium' (sodium) 0.6.13+mc1.21.4 with version 0.6.5 or later that is compatible with:
\t\t- vulkanmod 0.5.6
More details:
\t- Mod 'Sodium' (sodium) 0.6.13+mc1.21.4 is incompatible with any version of mod 'VulkanMod' (vulkanmod), yet a conflicting version is present: 0.5.6!
\tat net.fabricmc.loader.impl.FormattedException.ofLocalized(FormattedException.java:51)
");
        let d = analyze(&t).expect("Diagnose erwartet");
        assert!(d.cause.contains("Sodium"), "Ursache war: {}", d.cause);
        assert!(d.cause.contains("VulkanMod"), "Ursache war: {}", d.cause);
        assert!(d.confidence >= 90);
        // Fabrics eigener Vorschlag muss ganz oben stehen.
        assert!(d.suggestions[0].contains("Replace mod 'Sodium'"), "{:?}", d.suggestions);
    }

    /// Die Fortsetzungszeile darf die Ausnahme nicht verlieren.
    #[test]
    fn fortsetzungszeile_erbt_die_dringlichkeit() {
        let lines = parse("[10:00:00] [main/ERROR]: Kaputt\njava.lang.IllegalStateException: hier\n\tat foo.Bar.baz(Bar.java:1)");
        assert_eq!(lines[1].level, Level::Error);
        assert_eq!(lines[2].level, Level::Error);
        assert_eq!(root_cause(&lines).unwrap().class, "java.lang.IllegalStateException");
    }

    /// ... aber eine harmlose Zeile nach einem Fehler nicht mitreissen,
    /// sobald wieder ein eigener Zeitstempel kommt.
    #[test]
    fn eigener_praefix_beendet_die_fortsetzung() {
        let lines = parse("[10:00:00] [main/ERROR]: Kaputt\n[10:00:01] [main/INFO]: geht weiter");
        assert_eq!(lines[1].level, Level::Info);
    }

    #[test]
    fn mod_resolution_ohne_paar_faellt_auf_die_allgemeine_regel() {
        let t = log("
[22:31:26] [main/ERROR]: Mod resolution failed
[22:31:26] [main/INFO]: Reason: irgendwas anderes
");
        let d = analyze(&t).expect("Diagnose erwartet");
        assert!(d.cause.contains("Mod-Liste"), "Ursache war: {}", d.cause);
    }
}
