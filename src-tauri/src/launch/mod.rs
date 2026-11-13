//! Spielstart, Prozessverwaltung und Protokollauswertung.

pub mod args;
pub mod diagnose;
pub mod install;
pub mod official;
pub mod process;
pub mod telemetry;

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::auth::Account;
use crate::error::{Error, Result};
use crate::instance::InstanceConfig;
use crate::manifest::rules::{Feature, RuleContext};
use crate::manifest::VersionInfo;

pub use args::Substitutions;
pub use diagnose::{analyze, Diagnosis};
pub use process::{log_of, running, stop, RunningInstance};
pub use telemetry::Telemetry;

/// Wohin das Spiel direkt nach dem Start springen soll.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JoinTarget {
    None,
    Server { address: String },
    World { folder: String },
    Realm { realm_id: String },
}

impl Default for JoinTarget {
    fn default() -> Self {
        JoinTarget::None
    }
}

/// Kann diese Version direkt in eine Welt oder einen Server springen?
///
/// Statt an der Versionsnummer zu raten wird nachgesehen, ob das Manifest
/// die Quick-Play-Argumente ueberhaupt kennt. Das ist die einzige
/// verlaessliche Quelle -- Mojang hat es in 1.20 eingefuehrt, aber
/// Loader-Profile koennen davon abweichen.
pub fn supports_quick_play(info: &VersionInfo) -> bool {
    let Some(arguments) = &info.arguments else {
        return false;
    };
    arguments.game.iter().any(|entry| {
        serde_json::to_string(entry)
            .map(|s| s.contains("quickPlay"))
            .unwrap_or(false)
    })
}

/// Baut die vollstaendige Befehlszeile.
///
/// Gibt die Argumente ohne das Java-Programm selbst zurueck -- das
/// bestimmt der Aufrufer, weil es je Instanz ueberschrieben sein kann.
pub fn build_arguments(
    instance: &InstanceConfig,
    account: &Account,
    installed: &install::Installed,
    join: &JoinTarget,
    extra_jvm: &[String],
) -> Result<Vec<String>> {
    let info = &installed.info;
    let game_dir = instance.game_dir()?;
    let assets_dir = crate::paths::assets()?;

    let separator = if cfg!(windows) { ";" } else { ":" };
    let classpath = installed
        .classpath
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect::<Vec<_>>()
        .join(separator);

    let quick_play = supports_quick_play(info);

    let mut ctx = RuleContext::current()
        .with(Feature::IsDemoUser, false)
        .with(Feature::HasCustomResolution, false)
        .with(Feature::HasQuickPlaysSupport, quick_play);

    let mut subs = Substitutions::new()
        .set("auth_player_name", &account.username)
        .set("auth_uuid", account.uuid.replace('-', ""))
        .set("auth_access_token", &account.access_token)
        .set("auth_session", format!("token:{}", account.access_token))
        .set("user_type", if account.is_offline() { "legacy" } else { "msa" })
        .set("auth_xuid", "")
        .set("clientid", "")
        .set("version_name", &info.id)
        .set("version_type", if info.kind.is_empty() { "release" } else { &info.kind })
        .set("game_directory", game_dir.to_string_lossy())
        .set("assets_root", assets_dir.to_string_lossy())
        .set("game_assets", assets_dir.join("virtual").join("legacy").to_string_lossy())
        .set("assets_index_name", info.asset_index_id())
        .set("natives_directory", installed.natives_dir.to_string_lossy())
        .set("library_directory", crate::paths::libraries()?.to_string_lossy())
        .set("classpath", &classpath)
        .set("classpath_separator", separator)
        .set("launcher_name", "BuildMC")
        .set("launcher_version", env!("CARGO_PKG_VERSION"))
        .set("user_properties", "{}");

    // Beitrittsziel: ab 1.20 ueber Quick Play, davor ueber --server/--port.
    let mut legacy_join: Vec<String> = Vec::new();
    match join {
        JoinTarget::None => {}
        JoinTarget::Server { address } => {
            if quick_play {
                ctx = ctx.with(Feature::IsQuickPlayMultiplayer, true);
                subs = subs.set("quickPlayMultiplayer", address);
            } else {
                let (host, port) = crate::server::split_address(address);
                legacy_join = vec![
                    "--server".into(), host,
                    "--port".into(), port.to_string(),
                ];
            }
        }
        JoinTarget::World { folder } => {
            if !quick_play {
                return Err(Error::other(
                    "Diese Minecraft-Version kann nicht direkt in eine Welt springen",
                ));
            }
            ctx = ctx.with(Feature::IsQuickPlaySingleplayer, true);
            subs = subs.set("quickPlaySingleplayer", folder);
        }
        JoinTarget::Realm { realm_id } => {
            if !quick_play {
                return Err(Error::other(
                    "Diese Minecraft-Version kann nicht direkt in ein Realm springen",
                ));
            }
            ctx = ctx.with(Feature::IsQuickPlayRealms, true);
            subs = subs.set("quickPlayRealms", realm_id);
        }
    }

    let mut out: Vec<String> = Vec::new();

    // --- JVM ---
    let (min_ram, max_ram) = (instance.min_ram.min(instance.max_ram), instance.max_ram);
    out.push(format!("-Xms{min_ram}M"));
    out.push(format!("-Xmx{max_ram}M"));
    out.extend(extra_jvm.iter().cloned());
    out.extend(instance.jvm_args.iter().cloned());

    match &info.arguments {
        Some(a) => out.extend(args::expand(&a.jvm, &ctx, &subs)),
        None => {
            // Vor 1.13 nennt das Manifest keine JVM-Argumente.
            out.push(format!("-Djava.library.path={}", installed.natives_dir.display()));
            out.push("-cp".into());
            out.push(classpath.clone());
        }
    }

    // --- Hauptklasse ---
    out.push(info.main_class.clone());

    // --- Spiel ---
    match (&info.arguments, &info.legacy_arguments) {
        (Some(a), _) => out.extend(args::expand(&a.game, &ctx, &subs)),
        (None, Some(line)) => out.extend(args::expand_legacy(line, &subs)),
        (None, None) => {}
    }
    out.extend(legacy_join);

    Ok(out)
}

/// Sammelt die JVM-Schalter aus den Einstellungen.
pub fn jvm_flags_from_config(config: &crate::store::Config, java_major: u32) -> Vec<String> {
    let mut flags = Vec::new();

    if config.use_zgc && java_major >= 17 {
        flags.push("-XX:+UseZGC".into());
        // Der generationelle Modus war von 21 bis 23 ein eigener Schalter.
        // Ab 24 ist er die einzige Betriebsart, und der Schalter wurde
        // entfernt -- ihn dann noch zu setzen bringt nur eine Warnung.
        if (21..24).contains(&java_major) {
            flags.push("-XX:+ZGenerational".into());
        }
    }

    // Ab Java 24 warnt die Laufzeit bei jedem Zugriff auf native Funktionen,
    // und Minecraft nutzt sie ueber JNA und LWJGL staendig. Ohne diesen
    // Schalter steht das Protokoll voll damit -- und kuenftig wird es
    // nicht mehr nur eine Warnung sein.
    if java_major >= 24 {
        flags.push("--enable-native-access=ALL-UNNAMED".into());
    }

    flags.push("-XX:+AlwaysPreTouch".into());
    flags.push("-Dfile.encoding=UTF-8".into());
    flags
}

/// Sucht das Java, mit dem diese Instanz laufen soll.
pub fn resolve_java(instance: &InstanceConfig, config: &crate::store::Config, info: &VersionInfo)
    -> Result<crate::java::JavaInfo>
{
    let explicit = instance
        .java_path_override
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| Some(config.java_path.as_str()).filter(|s| !s.trim().is_empty()));

    crate::java::pick(info.required_java(), explicit)
}

/// Prueft, ob das Spielverzeichnis bereit ist.
pub fn ensure_game_dir(instance: &InstanceConfig) -> Result<std::path::PathBuf> {
    let dir = instance.game_dir()?;
    crate::paths::ensure_dir(&dir)?;
    for sub in ["mods", "resourcepacks", "shaderpacks", "saves", "config"] {
        crate::paths::ensure_dir(&dir.join(sub))?;
    }
    Ok(dir)
}

/// Nur fuer die Anzeige: die Befehlszeile lesbar machen, mit entferntem
/// Token. Ein Protokoll mit Zugriffstoken darf nicht weitergegeben werden.
pub fn redact(arguments: &[String], token: &str) -> Vec<String> {
    arguments
        .iter()
        .map(|a| {
            if !token.is_empty() && a.contains(token) {
                a.replace(token, "<token>")
            } else {
                a.clone()
            }
        })
        .collect()
}

/// Wo das Protokoll einer Instanz liegt.
pub fn log_path(instance_id: &str) -> Result<std::path::PathBuf> {
    Ok(crate::paths::logs()?.join(format!("{instance_id}.log")))
}

pub fn write_log_line(path: &Path, line: &str) {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(f, "{line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::Arguments;

    fn info_with(game: Vec<serde_json::Value>) -> VersionInfo {
        VersionInfo {
            id: "1.21.4".into(), kind: "release".into(),
            main_class: "net.minecraft.client.main.Main".into(),
            inherits_from: None, libraries: vec![],
            asset_index: None, assets: None, downloads: None,
            java_version: None,
            arguments: Some(Arguments { game, jvm: vec![] }),
            legacy_arguments: None, logging: None,
        }
    }

    #[test]
    fn quick_play_wird_am_manifest_erkannt() {
        let mit = info_with(vec![serde_json::json!({
            "rules": [{ "action": "allow",
                        "features": { "is_quick_play_multiplayer": true } }],
            "value": ["--quickPlayMultiplayer", "${quickPlayMultiplayer}"]
        })]);
        assert!(supports_quick_play(&mit));

        let ohne = info_with(vec![serde_json::json!("--username")]);
        assert!(!supports_quick_play(&ohne));
    }

    #[test]
    fn zgc_erst_ab_java_17() {
        let mut c = crate::store::Config::default();
        c.use_zgc = true;
        assert!(!jvm_flags_from_config(&c, 8).iter().any(|f| f.contains("UseZGC")));
        assert!(jvm_flags_from_config(&c, 17).iter().any(|f| f.contains("UseZGC")));
    }

    /// Im ersten echten Startversuch meldete Java 25:
    /// "Ignoring option ZGenerational; support was removed in 24.0".
    /// Der Schalter gilt nur von 21 bis 23.
    #[test]
    fn zgenerational_nur_von_21_bis_23() {
        let mut c = crate::store::Config::default();
        c.use_zgc = true;
        let hat = |v| jvm_flags_from_config(&c, v).iter().any(|f| f.contains("ZGenerational"));
        assert!(!hat(17), "vor 21 gibt es den Schalter nicht");
        assert!(hat(21));
        assert!(hat(23));
        assert!(!hat(24), "ab 24 entfernt -- würde nur warnen");
        assert!(!hat(25));
    }

    /// Ebenfalls aus dem echten Lauf: Java 25 warnte bei jedem Zugriff
    /// von JNA und LWJGL auf native Funktionen.
    #[test]
    fn nativer_zugriff_wird_ab_24_freigegeben() {
        let c = crate::store::Config::default();
        let hat = |v| jvm_flags_from_config(&c, v)
            .iter().any(|f| f.contains("enable-native-access"));
        assert!(!hat(21));
        assert!(hat(24));
        assert!(hat(25));
    }

    #[test]
    fn token_wird_aus_der_anzeige_entfernt() {
        let args = vec!["--accessToken".into(), "geheim123".into(), "--x".into()];
        let safe = redact(&args, "geheim123");
        assert_eq!(safe[1], "<token>");
        assert!(!safe.join(" ").contains("geheim123"));
    }

    #[test]
    fn leeres_token_aendert_nichts() {
        let args = vec!["--a".into(), "--b".into()];
        assert_eq!(redact(&args, ""), args);
    }
}

/* ---- Ablauf ------------------------------------------------------------ */

/// Startet eine Instanz von Anfang bis Ende.
///
/// Die Reihenfolge ist so gewaehlt, dass alles, was scheitern kann, vor
/// dem ersten Download geprueft wird: ein fehlendes Konto oder ein zu
/// altes Java sollen nicht erst nach einer Viertelstunde Ladezeit
/// auffallen.
pub async fn start(
    instance_id: &str,
    join: JoinTarget,
    sink: std::sync::Arc<dyn crate::download::Sink>,
    events: std::sync::Arc<dyn process::Events>,
) -> Result<process::RunningInstance> {
    if process::is_running(instance_id) {
        return Err(Error::AlreadyRunning);
    }

    // Das Protokoll des letzten Laufs weg -- sonst mischt es sich mit dem neuen.
    process::clear_log(instance_id);
    process::note(instance_id, "Start wird vorbereitet", &events);

    let mut instance = crate::instance::load(instance_id)?;
    let config = crate::store::load_config();

    // 1. Konto
    let accounts = crate::store::load_accounts();
    let picked = accounts
        .iter()
        .find(|a| a.active)
        .or_else(|| accounts.first())
        .ok_or(Error::NoAccount)?
        .clone();

    // Ein abgelaufenes Zugriffstoken faellt sonst erst *im* Spiel auf --
    // als 401 bei Profil, Realms und Serverbeitritt. Das Spiel selbst kann
    // es nicht erneuern (es bekommt nie den refresh_token), nur der
    // Launcher kann das vorher tun.
    //
    // Schlaegt die Erneuerung fehl -- keine Azure-Anwendungskennung
    // hinterlegt, Netzwerk weg, refresh_token abgelehnt -- darf das den
    // Start nicht verhindern. Vorher lief das Spiel in diesem Fall
    // wenigstens mit dem alten Token weiter (nur einzelne Online-Dinge
    // schlagen dann fehl); den Start deswegen ganz zu blockieren waere
    // schlechter als der Zustand davor, nicht besser.
    let account = match crate::auth::microsoft::ensure_fresh(&picked).await {
        Ok(a) => a,
        Err(e) => {
            process::note(
                instance_id,
                &format!("Zugriffstoken konnte nicht erneuert werden ({e}) -- Start läuft mit dem                           bestehenden Token weiter, einzelne Online-Funktionen können fehlschlagen"),
                &events,
            );
            picked.clone()
        }
    };
    if account.access_token != picked.access_token {
        process::note(instance_id, "Zugriffstoken wurde erneuert", &events);
        // Erst jetzt, nach dem Netzwerkaufruf, sperren und frisch laden --
        // sonst wuerde der Stand von vor der Erneuerung zurueckgeschrieben
        // und eine zwischenzeitliche Aenderung waere spurlos weg (etwa ein
        // zweiter Start, der dasselbe Konto parallel erneuert hat).
        crate::store::with_accounts(|accounts| {
            for a in accounts.iter_mut() {
                if a.id == account.id {
                    *a = account.clone();
                }
            }
            Ok(())
        })?;
    }

    process::note(instance_id, &format!("Konto: {}", account.username), &events);

    // 2. Versions-JSON -- sie sagt, welches Java gebraucht wird
    let version_id = instance.launch_version();
    process::note(instance_id, &format!("Version {version_id} wird aufgelöst"), &events);
    let info = crate::manifest::resolve(&version_id).await?;

    // 3. Java vor dem Download pruefen
    //
    // Auf einem frischen Rechner ohne installiertes Java faende `pick()`
    // sonst nichts und der Start bräche mit einer Fehlermeldung ab, die den
    // Nutzer alleine laesst -- die fertige Auto-Installation
    // (`java::install::install`) existierte zwar schon, war aber nirgends
    // verdrahtet. Nur bei genau diesem Fehler (kein Java gefunden, keine
    // eigene Java-Wahl hinterlegt) automatisch nachladen -- ein falsch
    // gesetzter eigener Pfad soll weiterhin als Fehler auffallen, nicht
    // stillschweigend uebergangen werden.
    let java = match resolve_java(&instance, &config, &info) {
        Ok(j) => j,
        Err(Error::NoJava(major)) => {
            process::note(
                instance_id,
                &format!("Kein passendes Java gefunden -- Java {major} wird heruntergeladen"),
                &events,
            );
            crate::java::install::install(major).await?
        }
        Err(e) => return Err(e),
    };
    process::note(
        instance_id,
        &format!("Java {} ({}), benötigt {}", java.version, java.path, info.required_java()),
        &events,
    );

    // 4. Herunterladen
    process::note(instance_id, "Dateien werden geprüft", &events);
    let installed = install::ensure(
        &version_id,
        config.concurrent_downloads,
        config.verify_sha1,
        sink,
    )
    .await?;

    // 5. Verzeichnisse und Argumente
    let game_dir = ensure_game_dir(&instance)?;
    process::note(
        instance_id,
        &format!("Spielverzeichnis: {}", game_dir.display()),
        &events,
    );
    let flags = jvm_flags_from_config(&config, java.major);
    let arguments = build_arguments(&instance, &account, &installed, &join, &flags)?;

    // 6. Sicherheitspruefung -- jede .jar im Mods-Ordner, nicht nur was
    // gerade heruntergeladen wurde. Ohne Schluessel und ohne ClamAV bleibt
    // sie folgenlos, kein Fehler nur weil nichts geprueft werden konnte.
    //
    // Scheitert die Pruefung selbst -- Datei nicht lesbar, Zwischenspeicher
    // nicht schreibbar, o.ae. -- ist das kein Fund und darf den Start nicht
    // blockieren, auch nicht im Block-Modus: eine ausgefallene Pruefung
    // sagt nichts ueber die Datei aus, im Unterschied zu einem echten
    // Fund. Sonst wiederholt sich genau der Fehler, der schon einmal den
    // ganzen Start an einer eigentlich harmlosen Nebensache scheitern liess
    // (siehe die Azure-Anwendungskennung weiter oben).
    if config.security_auto_scan {
        process::note(instance_id, "Mods werden geprüft", &events);
        let funde = match crate::security::verify_instance(instance_id).await {
            Ok(f) => f,
            Err(e) => {
                process::note(
                    instance_id,
                    &format!("Sicherheitsprüfung konnte nicht durchgeführt werden ({e}) -- Start läuft ohne diese Prüfung weiter"),
                    &events,
                );
                Vec::new()
            }
        };
        if !funde.is_empty() {
            for f in &funde {
                process::note(
                    instance_id,
                    &format!("Sicherheitshinweis: {} -- {}", f.filename, f.reason),
                    &events,
                );
            }
            if config.security_mode == crate::security::Mode::Block {
                let namen: Vec<_> = funde.iter().map(|f| f.filename.as_str()).collect();
                return Err(Error::other(format!(
                    "Start verweigert -- auffällig: {}. In den Sicherheitseinstellungen                      kann der Fund bewusst freigegeben werden.",
                    namen.join(", ")
                )));
            }
        }
    }

    // 7. Starten
    process::note(
        instance_id,
        &format!("{} Bibliotheken im Klassenpfad, {} MB zugewiesen",
                 installed.classpath.len(), instance.max_ram),
        &events,
    );
    process::note(instance_id, "Java wird gestartet", &events);

    let running = process::spawn(
        instance_id,
        std::path::Path::new(&java.path),
        &arguments,
        &game_dir,
        events,
    )?;

    // 7. Zaehler fortschreiben
    instance.last_played = crate::instance::now();
    instance.launch_count += 1;
    crate::instance::save(&instance)?;

    Ok(running)
}

/// Schreibt die Spielzeit fort, wenn ein Spiel endet.
pub fn record_playtime(instance_id: &str, seconds: u64) {
    let Ok(mut instance) = crate::instance::load(instance_id) else { return };
    instance.play_seconds += seconds;
    let _ = crate::instance::save(&instance);
}

/// Wertet das Protokoll einer Instanz aus.
///
/// Erst der Speicher, dann die Datei -- nach einem Neustart des Launchers
/// ist der Puffer leer, die Datei aber noch da.
pub fn analyze_instance(instance_id: &str) -> Result<Option<Diagnosis>> {
    let from_memory = process::log_of(instance_id).join("\n");
    if !from_memory.trim().is_empty() {
        return Ok(analyze(&from_memory));
    }
    let path = log_path(instance_id)?;
    if !path.exists() {
        return Ok(None);
    }
    Ok(analyze(&std::fs::read_to_string(path)?))
}

/// Schlaegt JVM-Schalter vor.
///
/// Abgeleitet aus dem zugewiesenen Speicher, der Java-Fassung und der
/// Anzahl der Mods -- keine feste Liste, die ueberall gleich waere.
pub fn suggest_flags(instance: &InstanceConfig, java_major: u32) -> Vec<String> {
    let mut out = vec![
        format!("-Xms{}M", instance.min_ram),
        format!("-Xmx{}M", instance.max_ram),
    ];

    if java_major >= 17 {
        out.push("-XX:+UseZGC".into());
        if java_major >= 21 {
            out.push("-XX:+ZGenerational".into());
        }
    } else {
        // Vor 17 ist G1 die beste verfuegbare Wahl.
        out.push("-XX:+UseG1GC".into());
        out.push("-XX:MaxGCPauseMillis=50".into());
    }

    out.push("-XX:+AlwaysPreTouch".into());

    // Viele Mods laden sehr viele Klassen.
    if instance.mods.len() > 60 {
        out.push("-XX:MaxMetaspaceSize=512m".into());
    }

    // Grosse Halden profitieren von grossen Speicherseiten.
    if instance.max_ram >= 8192 {
        out.push("-XX:+UseTransparentHugePages".into());
    }

    out
}

#[cfg(test)]
mod flag_tests {
    use super::*;

    fn instance(mods: usize, max_ram: u32) -> InstanceConfig {
        InstanceConfig {
            mods: (0..mods).map(|i| format!("m{i}.jar")).collect(),
            min_ram: 2048,
            max_ram,
            ..Default::default()
        }
    }

    #[test]
    fn alte_java_fassung_bekommt_g1() {
        let f = suggest_flags(&instance(10, 4096), 8);
        assert!(f.iter().any(|x| x.contains("UseG1GC")));
        assert!(!f.iter().any(|x| x.contains("UseZGC")));
    }

    #[test]
    fn viele_mods_bekommen_mehr_klassenspeicher() {
        assert!(!suggest_flags(&instance(10, 4096), 21)
            .iter().any(|x| x.contains("Metaspace")));
        assert!(suggest_flags(&instance(96, 4096), 21)
            .iter().any(|x| x.contains("Metaspace")));
    }

    #[test]
    fn grosse_halde_bekommt_grosse_speicherseiten() {
        assert!(!suggest_flags(&instance(10, 6144), 21)
            .iter().any(|x| x.contains("HugePages")));
        assert!(suggest_flags(&instance(10, 8192), 21)
            .iter().any(|x| x.contains("HugePages")));
    }
}
