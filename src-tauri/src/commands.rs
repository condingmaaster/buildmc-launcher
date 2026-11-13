//! Die Befehle, die die Oberflaeche aufruft.
//!
//! Namen und Argumente sind aus Version 0.1.3 uebernommen, soweit es sie
//! dort gab -- die wiederhergestellte Schnittstellenbeschreibung liegt in
//! `recovered/spec/api-contract.md`.

use tauri::AppHandle;

use crate::auth::Account;
use crate::bridge::{TauriBedrockEvents, TauriEvents, TauriRustEvents, TauriSink};
use crate::error::Result;
use crate::instance::InstanceConfig;
use crate::java::JavaInfo;
use crate::launch::{process::RunningInstance, Diagnosis, JoinTarget};
use crate::manifest::VersionEntry;
use crate::server::SavedServer;
use crate::store::Config;

/* ---- Einstellungen ---------------------------------------------------- */

#[tauri::command]
pub fn get_config() -> Config {
    crate::store::load_config()
}

#[tauri::command]
pub fn save_config(config: Config) -> Result<()> {
    crate::store::save_config(&config)
}

#[tauri::command]
pub fn get_system_memory() -> u64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.total_memory() / 1024 / 1024 // MB
}

/* ---- Instanzen -------------------------------------------------------- */

#[tauri::command]
pub fn list_instances() -> Result<Vec<InstanceConfig>> {
    crate::instance::list()
}

#[tauri::command]
pub fn create_instance(
    name: String,
    mc_version: String,
    loader: String,
    use_custom_ui: Option<bool>,
) -> Result<InstanceConfig> {
    crate::instance::create(&name, &mc_version, &loader, use_custom_ui.unwrap_or(false))
}

#[tauri::command]
pub fn update_instance(config: InstanceConfig) -> Result<InstanceConfig> {
    crate::instance::save(&config)?;
    Ok(config)
}

#[tauri::command]
pub fn open_instance_folder(id: String) -> Result<()> {
    let dir = crate::paths::instance_game_dir(&id)?;
    crate::paths::ensure_dir(&dir)?;
    open::that_detached(&dir)
        .map_err(|e| crate::Error::other(format!("Ordner ließ sich nicht öffnen: {e}")))
}

/// Oeffnet eine Adresse im Standardbrowser -- fuer die Microsoft-Anmeldung
/// und andere Verweise nach draussen. Nur http/https, alles andere waere
/// ein Weg, beliebige lokale Dateien oder Programme zu starten.
#[tauri::command]
pub fn open_external(url: String) -> Result<()> {
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err(crate::Error::other("Nur http/https-Adressen lassen sich öffnen"));
    }
    open::that_detached(&url)
        .map_err(|e| crate::Error::other(format!("Adresse ließ sich nicht öffnen: {e}")))
}

/* ---- Versionen und Java ----------------------------------------------- */

#[tauri::command]
pub async fn get_versions() -> Result<Vec<VersionEntry>> {
    crate::manifest::list_versions().await
}

#[tauri::command]
pub async fn download_version(app: AppHandle, mc_version: String) -> Result<()> {
    let config = crate::store::load_config();
    crate::launch::install::ensure(
        &mc_version,
        config.concurrent_downloads,
        config.verify_sha1,
        TauriSink::new(app, "download-progress"),
    )
    .await?;
    Ok(())
}

#[tauri::command]
pub fn detect_java(mc_version: Option<String>) -> Vec<JavaInfo> {
    // Die Mindestanforderung steht in der Versions-JSON; liegt sie noch
    // nicht vor, werden alle gefundenen Installationen gemeldet.
    let required = mc_version
        .and_then(|v| crate::manifest::read_local(&v).ok())
        .map(|i| i.required_java())
        .unwrap_or(0);

    crate::java::discover()
        .into_iter()
        .filter(|j| j.major >= required)
        .collect()
}

#[tauri::command]
pub fn check_java_path(java_path: String) -> Result<JavaInfo> {
    crate::java::probe(std::path::Path::new(&java_path))
}

/* ---- Starten ---------------------------------------------------------- */

#[tauri::command]
pub async fn launch_instance(
    app: AppHandle,
    instance_id: String,
    join: Option<JoinTarget>,
) -> Result<RunningInstance> {
    crate::launch::start(
        &instance_id,
        join.unwrap_or_default(),
        TauriSink::new(app.clone(), "download-progress"),
        TauriEvents::new(app),
    )
    .await
}

/// Legt die Instanz als Profil im offiziellen Minecraft-Launcher an und öffnet ihn.
#[tauri::command]
pub async fn launch_in_official_launcher(instance_id: String) -> Result<String> {
    let instance = crate::instance::load(&instance_id)?;
    crate::launch::official::install_profile(&instance).await?;
    Ok(if crate::launch::official::open_launcher() {
        "Das Profil liegt im Minecraft-Launcher bereit. Dort auf „Spielen“ drücken.".to_string()
    } else {
        "Das Profil ist angelegt, aber der Minecraft-Launcher wurde nicht gefunden. Installiere ihn (unter Linux zum Beispiel per Flatpak: com.mojang.Minecraft), melde dich dort an und wähle das Profil „BuildMC – …“.".to_string()
    })
}

#[tauri::command]
pub fn stop_instance(instance_id: String) -> Result<()> {
    crate::launch::stop(&instance_id)
}

#[tauri::command]
pub fn get_running_instances() -> Vec<RunningInstance> {
    crate::launch::running()
}

#[tauri::command]
pub fn get_launch_log(instance_id: String) -> Vec<String> {
    crate::launch::log_of(&instance_id)
}

#[tauri::command]
pub fn clear_launch_log(instance_id: String) -> Result<()> {
    crate::launch::process::clear_log(&instance_id);
    let path = crate::launch::log_path(&instance_id)?;
    let _ = std::fs::remove_file(path);
    Ok(())
}

#[tauri::command]
pub fn analyze_log(instance_id: String) -> Result<Option<Diagnosis>> {
    crate::launch::analyze_instance(&instance_id)
}

#[tauri::command]
pub fn analyze_log_text(text: String) -> Option<Diagnosis> {
    crate::launch::analyze(&text)
}

#[tauri::command]
pub fn suggest_jvm_flags(instance_id: String) -> Result<Vec<String>> {
    let instance = crate::instance::load(&instance_id)?;
    let java_major = crate::java::discover().first().map(|j| j.major).unwrap_or(21);
    Ok(crate::launch::suggest_flags(&instance, java_major))
}

#[tauri::command]
pub fn list_worlds(instance_id: String) -> Result<Vec<serde_json::Value>> {
    let saves = crate::paths::instance_game_dir(&instance_id)?.join("saves");
    if !saves.exists() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(saves)? {
        let Ok(entry) = entry else { continue };
        if !entry.path().is_dir() {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339())
            .unwrap_or_default();
        out.push(serde_json::json!({
            "folder": folder.clone(),
            "name": folder.replace('_', " "),
            "last_played": modified,
        }));
    }
    Ok(out)
}

/* ---- Konten ----------------------------------------------------------- */

#[tauri::command]
pub fn get_accounts() -> Vec<Account> {
    crate::store::load_accounts()
}

#[tauri::command]
pub fn add_offline_account(username: String) -> Result<Account> {
    crate::auth::offline::validate_username(&username)?;
    let uuid = crate::auth::offline::offline_uuid(&username);

    crate::store::with_accounts(|accounts| {
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            username: username.trim().to_string(),
            uuid: uuid.to_string(),
            account_type: "offline".into(),
            active: accounts.is_empty(),
            ..Default::default()
        };
        accounts.push(account.clone());
        Ok(account)
    })
}

#[tauri::command]
pub fn set_active_account(account_id: String) -> Result<()> {
    crate::store::with_accounts(|accounts| {
        for a in accounts.iter_mut() {
            a.active = a.id == account_id;
        }
        Ok(())
    })
}

#[tauri::command]
pub fn remove_account(account_id: String) -> Result<()> {
    crate::store::with_accounts(|accounts| {
        accounts.retain(|a| a.id != account_id);
        // Ohne aktives Konto das erste uebrige waehlen.
        if !accounts.iter().any(|a| a.active) {
            if let Some(first) = accounts.first_mut() {
                first.active = true;
            }
        }
        Ok(())
    })
}

/* ---- Server ----------------------------------------------------------- */

#[tauri::command]
pub fn list_servers() -> Vec<SavedServer> {
    crate::store::load_servers()
}

#[tauri::command]
pub fn save_server(name: String, address: String) -> Result<SavedServer> {
    let mut servers = crate::store::load_servers();
    let server = SavedServer {
        id: uuid::Uuid::new_v4().to_string(),
        name: if name.trim().is_empty() { address.clone() } else { name },
        address,
        favorite: false,
    };
    servers.push(server.clone());
    crate::store::save_servers(&servers)?;
    Ok(server)
}

#[tauri::command]
pub fn delete_server(id: String) -> Result<()> {
    let mut servers = crate::store::load_servers();
    servers.retain(|s| s.id != id);
    crate::store::save_servers(&servers)
}

/* ---- Assistent -------------------------------------------------------- */

#[tauri::command]
pub fn ai_status() -> crate::ai::AiStatus {
    crate::ai::status(crate::store::load_config().model())
}

#[tauri::command]
pub fn ai_set_key(key: Option<String>) -> Result<crate::ai::AiStatus> {
    crate::ai::keys::store(key.as_deref())?;
    Ok(ai_status())
}

#[tauri::command]
pub fn curseforge_status() -> crate::curseforge::Status {
    crate::curseforge::status()
}

#[tauri::command]
pub fn curseforge_set_key(key: Option<String>) -> Result<crate::curseforge::Status> {
    crate::ai::keys::store_curseforge_key(key.as_deref())?;
    Ok(crate::curseforge::status())
}

/* ---- Sicherheit --------------------------------------------------------
   Pflichtpruefung vor jedem Start (siehe launch::start). Diese Befehle
   sind fuer das Verwaltungscenter -- Status ansehen, Schluessel setzen,
   von Hand neu pruefen, einen Fehlalarm bewusst uebergehen.          */

#[tauri::command]
pub fn security_status() -> crate::security::Status {
    crate::security::status()
}

#[tauri::command]
pub fn security_set_key(key: Option<String>) -> Result<crate::security::Status> {
    crate::security::keys::store(key.as_deref())?;
    Ok(crate::security::status())
}

#[tauri::command]
pub fn security_set_mode(mode: crate::security::Mode) -> Result<()> {
    let mut config = crate::store::load_config();
    config.security_mode = mode;
    crate::store::save_config(&config)
}

#[tauri::command]
pub fn security_set_auto_scan(enabled: bool) -> Result<()> {
    let mut config = crate::store::load_config();
    config.security_auto_scan = enabled;
    crate::store::save_config(&config)
}

#[tauri::command]
pub async fn security_scan_instance(instance_id: String) -> Result<Vec<crate::security::FileVerdict>> {
    crate::security::verify_instance(&instance_id).await
}

#[tauri::command]
pub fn security_history() -> Vec<crate::security::FileVerdict> {
    crate::security::history()
}

#[tauri::command]
pub fn security_allow_hash(sha1: String) -> Result<()> {
    crate::security::allow_hash(&sha1)
}

/// Laedt eine bislang unbekannte Datei hoch -- nur auf ausdrueckliche
/// Bitte, nie automatisch (siehe `security::virustotal::upload`).
#[tauri::command]
pub async fn security_upload_unknown(instance_id: String, filename: String) -> Result<()> {
    let path = crate::paths::safe_join(&crate::mods::mods_dir(&instance_id)?, &filename)?;
    crate::security::virustotal::upload(&path).await
}

#[tauri::command]
pub fn ai_set_model(model: String) -> Result<()> {
    let mut config = crate::store::load_config();
    config.ai_model = model;
    crate::store::save_config(&config)
}

#[tauri::command]
pub async fn ai_list_models() -> Result<Vec<crate::ai::ModelInfo>> {
    let key = crate::ai::keys::api_key()
        .ok_or_else(|| crate::Error::other("Kein Schlüssel hinterlegt"))?;
    crate::ai::list_models(&key).await
}

/* ---- Mods -------------------------------------------------------------- */

/// Sucht bei Modrinth und, wenn eingeschaltet und ein Schluessel
/// hinterlegt ist, zugleich bei CurseForge -- gleichzeitig, nicht
/// nacheinander, sonst wartet die Oberflaeche auf den langsameren der
/// beiden. Ohne CurseForge-Schluessel liefert dessen Anfrage schlicht
/// eine leere Seite, kein Fehler.
#[tauri::command]
pub async fn search_mods(
    query: Option<String>,
    game_version: Option<String>,
    loader: Option<String>,
    project_type: Option<String>,
    page: Option<u32>,
) -> Result<crate::modrinth::SearchPage> {
    let q = query.unwrap_or_default();
    let kind = project_type.unwrap_or_else(|| "mod".into());
    let seite = page.unwrap_or(0);

    let modrinth_fut =
        crate::modrinth::search(&q, game_version.as_deref(), loader.as_deref(), &kind, seite);

    if !crate::store::load_config().curseforge_enabled {
        return modrinth_fut.await;
    }

    let curseforge_fut =
        crate::curseforge::search_in(&q, game_version.as_deref(), loader.as_deref(), &kind, seite);

    let (modrinth, curseforge) = tokio::join!(modrinth_fut, curseforge_fut);
    let mut modrinth = modrinth?;
    // Ein CurseForge-Fehlschlag (abgelehnter Schluessel, Netzwerk) darf
    // Modrinth-Treffer nicht mit hinunterreissen -- lieber die eine
    // Haelfte zeigen als gar nichts.
    if let Ok(cf) = curseforge {
        modrinth.total += cf.total;
        modrinth.results.extend(cf.results);
    }
    Ok(modrinth)
}

/// `provider` fehlt bei alten Aufrufen -- dann gilt Modrinth, wie es vor
/// CurseForge immer der Fall war.
#[tauri::command]
pub async fn get_mod_versions(
    project_id: String,
    game_version: Option<String>,
    loader: Option<String>,
    provider: Option<String>,
) -> Result<Vec<crate::modrinth::ModVersion>> {
    match provider.as_deref() {
        Some("curseforge") => {
            crate::curseforge::versions(&project_id, game_version.as_deref(), loader.as_deref())
                .await
        }
        _ => crate::modrinth::versions(&project_id, game_version.as_deref(), loader.as_deref()).await,
    }
}

#[tauri::command]
pub async fn list_instance_mods(instance_id: String) -> Result<Vec<crate::mods::InstalledMod>> {
    let mut mods = crate::mods::list(&instance_id)?;
    // Fehlende Symbole nachholen; scheitert das Netz, bleibt die Liste
    // trotzdem vollstaendig.
    let _ = crate::mods::enrich(&instance_id, &mut mods).await;
    Ok(mods)
}

#[tauri::command]
pub async fn install_mod(
    app: AppHandle,
    instance_id: String,
    version_id: String,
    provider: Option<String>,
) -> Result<crate::mods::InstalledMod> {
    crate::mods::install(
        &instance_id,
        &version_id,
        provider.as_deref(),
        TauriSink::new(app, "download-progress"),
    )
    .await
}

/// Shaderpakete werden als ZIP in shaderpacks abgelegt, nicht als Mod.
#[tauri::command]
pub async fn install_shader_pack(
    app: AppHandle,
    instance_id: String,
    version_id: String,
) -> Result<String> {
    crate::packs::install_shader(
        &instance_id, &version_id, TauriSink::new(app, "download-progress"),
    ).await
}

#[tauri::command]
pub fn remove_mod(instance_id: String, mod_filename: String) -> Result<()> {
    crate::mods::remove(&instance_id, &mod_filename)
}

#[tauri::command]
pub fn toggle_mod(instance_id: String, mod_filename: String) -> Result<String> {
    crate::mods::toggle(&instance_id, &mod_filename)
}

#[tauri::command]
pub fn add_local_mod(
    instance_id: String,
    source_path: String,
) -> Result<crate::mods::InstalledMod> {
    crate::mods::add_local(&instance_id, std::path::Path::new(&source_path))
}

#[tauri::command]
pub async fn check_mod_updates(instance_id: String) -> Result<Vec<crate::mods::InstalledMod>> {
    let instance = crate::instance::load(&instance_id)?;
    crate::mods::check_updates(&instance_id, &instance.mc_version, &instance.loader).await
}

/// Aktualisiert alle Mods, fuer die eine neuere Fassung vorliegt.
///
/// Die alte Datei wird erst entfernt, wenn die neue liegt -- ein Abbruch
/// mittendrin laesst sonst eine Instanz ohne die Mod zurueck.
#[tauri::command]
pub async fn update_all_mods(app: AppHandle, instance_id: String) -> Result<u32> {
    let instance = crate::instance::load(&instance_id)?;
    let outdated =
        crate::mods::check_updates(&instance_id, &instance.mc_version, &instance.loader).await?;

    let mut done = 0;
    for m in outdated.iter().filter(|m| m.update_available.is_some()) {
        let (Some(project_id), Some(old_version)) = (&m.project_id, &m.version_id) else {
            continue;
        };

        let available = crate::modrinth::versions(
            project_id,
            Some(&instance.mc_version),
            Some(&instance.loader),
        )
        .await
        .unwrap_or_default();

        let Some(newest) = available
            .iter()
            .find(|v| v.release_type == "release")
            .or_else(|| available.first())
        else {
            continue;
        };
        if &newest.version_id == old_version {
            continue;
        }

        crate::mods::install(
            &instance_id,
            &newest.version_id,
            Some(&newest.provider),
            TauriSink::new(app.clone(), "download-progress"),
        )
        .await?;

        // Erst jetzt die alte Datei weg -- und nur, wenn sie anders heisst.
        if newest.filename != m.filename {
            let _ = crate::mods::remove(&instance_id, &m.filename);
        }
        done += 1;
    }

    Ok(done)
}

/// Plant eine Installation, bevor etwas geschrieben wird.
///
/// Zeigt, welche Abhaengigkeiten mitkommen, was schon da ist und was sich
/// stoesst. Ohne diesen Schritt merkt der Nutzer erst beim Start, dass
/// eine Mod eine andere braucht -- und sucht den Fehler bei sich.
#[tauri::command]
pub async fn resolve_mod_install(
    instance_id: String,
    version_id: String,
) -> Result<crate::mods::plan::ResolvePlan> {
    let instance = crate::instance::load(&instance_id)?;
    crate::mods::plan::resolve(&instance_id, &version_id, &instance.mc_version, &instance.loader)
        .await
}

/// Setzt einen zuvor geprueften Plan um.
#[tauri::command]
pub async fn apply_mod_plan(
    app: AppHandle,
    instance_id: String,
    version_ids: Vec<String>,
) -> Result<u32> {
    let mut done = 0;
    for id in version_ids {
        // Nur die Kennung kommt hier an, kein ModVersion-Objekt mehr --
        // das Format allein verraet den Anbieter: Modrinth-Kennungen
        // tragen nie einen Doppelpunkt, CurseForges zusammengesetzte immer.
        let provider = id.contains(':').then_some("curseforge");
        crate::mods::install(
            &instance_id,
            &id,
            provider,
            TauriSink::new(app.clone(), "download-progress"),
        )
        .await?;
        done += 1;
    }
    Ok(done)
}

/* ---- Loader ------------------------------------------------------------ */

#[tauri::command]
pub async fn get_loader_versions(
    loader: String,
    mc_version: String,
) -> Result<Vec<crate::loader::LoaderVersion>> {
    let kind = crate::loader::LoaderKind::parse(&loader)
        .ok_or_else(|| crate::Error::other(format!("{loader} ist kein Mod-Loader")))?;
    crate::loader::versions(kind, &mc_version).await
}

/// Installiert den Loader und traegt ihn in die Instanz ein.
///
/// Ohne den Eintrag wuerde die Instanz weiterhin die Grundversion starten
/// und der Loader waere zwar abgelegt, aber wirkungslos.
#[tauri::command]
pub async fn install_loader(
    instance_id: String,
    loader: String,
    version: Option<String>,
) -> Result<crate::instance::InstanceConfig> {
    let kind = crate::loader::LoaderKind::parse(&loader)
        .ok_or_else(|| crate::Error::other(format!("{loader} ist kein Mod-Loader")))?;

    let mut instance = crate::instance::load(&instance_id)?;

    // Ohne Angabe die neueste stabile Fassung.
    let loader_version = match version {
        Some(v) if !v.trim().is_empty() => v,
        _ => crate::loader::recommended(kind, &instance.mc_version).await?,
    };

    crate::loader::install(kind, &instance.mc_version, &loader_version).await?;

    instance.loader = loader.to_lowercase();
    instance.fabric_version = Some(loader_version);
    crate::instance::save(&instance)?;
    Ok(instance)
}

/* ---- Instanzverwaltung -------------------------------------------------- */

#[tauri::command]
pub fn trash_instance(id: String) -> Result<()> { crate::instance::trash(&id) }

#[tauri::command]
pub fn restore_instance(id: String) -> Result<()> { crate::instance::restore(&id) }

#[tauri::command]
pub fn permanently_delete_instance(id: String) -> Result<()> { crate::instance::purge(&id) }

#[tauri::command]
pub fn list_trashed_instances() -> Result<Vec<InstanceConfig>> { crate::instance::list_trashed() }

#[tauri::command]
pub fn duplicate_instance(id: String, name: Option<String>) -> Result<InstanceConfig> {
    crate::instance::duplicate(&id, name.as_deref().unwrap_or(""))
}

#[tauri::command]
pub fn set_instance_group(id: String, group: Option<String>) -> Result<()> {
    crate::instance::set_group(&id, group)
}

/* ---- Server -------------------------------------------------------------- */

#[tauri::command]
pub async fn ping_server(address: String) -> crate::server::ServerStatus {
    // Die Abfrage blockiert; in einen eigenen Faden, damit die Oberflaeche
    // waehrenddessen ansprechbar bleibt.
    tokio::task::spawn_blocking(move || crate::server::ping(&address))
        .await
        .unwrap_or_else(|_| crate::server::ping("")) // liefert "offline"
}

/* ---- Sicherungen --------------------------------------------------------- */

#[tauri::command]
pub fn list_snapshots(instance_id: String) -> Vec<crate::snapshot::Snapshot> {
    crate::snapshot::list(&instance_id)
}

#[tauri::command]
pub async fn create_snapshot(
    instance_id: String,
    label: String,
    include_worlds: Option<bool>,
) -> Result<crate::snapshot::Snapshot> {
    let worlds = include_worlds.unwrap_or(true);
    let snapshot = tokio::task::spawn_blocking(move || {
        crate::snapshot::create(&instance_id, &label, worlds, false)
    })
    .await
    .map_err(|e| crate::Error::other(e.to_string()))??;

    Ok(snapshot)
}

#[tauri::command]
pub async fn restore_snapshot(instance_id: String, snapshot_id: String) -> Result<()> {
    tokio::task::spawn_blocking(move || crate::snapshot::restore(&instance_id, &snapshot_id))
        .await
        .map_err(|e| crate::Error::other(e.to_string()))?
}

#[tauri::command]
pub fn delete_snapshot(instance_id: String, snapshot_id: String) -> Result<()> {
    crate::snapshot::delete(&instance_id, &snapshot_id)
}

/* ---- Laufzeit ------------------------------------------------------------ */

#[tauri::command]
pub fn get_telemetry(instance_id: String) -> Result<crate::launch::Telemetry> {
    crate::launch::telemetry::sample(&instance_id)
        .ok_or_else(|| crate::Error::not_found("Diese Instanz läuft nicht"))
}

#[tauri::command]
pub fn list_crash_reports(instance_id: String) -> Result<Vec<serde_json::Value>> {
    // Ein Absturz liegt vor, wenn das Protokoll eine Diagnose hergibt.
    match crate::launch::analyze_instance(&instance_id)? {
        None => Ok(Vec::new()),
        Some(d) => {
            let excerpt = crate::launch::log_of(&instance_id);
            let tail = excerpt.iter().rev().take(8).rev().cloned().collect::<Vec<_>>().join("\n");
            Ok(vec![serde_json::json!({
                "instance_id": instance_id,
                "occurred_at": crate::instance::now(),
                "exit_code": 1,
                "signature": d.trigger_line,
                "diagnosis": d,
                "log_excerpt": tail,
            })])
        }
    }
}

/* ---- Java ---------------------------------------------------------------- */

#[tauri::command]
pub fn list_installed_java() -> Vec<JavaInfo> { crate::java::discover() }

/* ---- Modrinth ------------------------------------------------------------ */

#[tauri::command]
pub async fn get_project_info(project_id: String) -> Result<serde_json::Value> {
    let found = crate::modrinth::projects(&[project_id.clone()]).await?;
    let (title, icon) = found
        .get(&project_id)
        .cloned()
        .ok_or_else(|| crate::Error::not_found("Projekt nicht gefunden"))?;
    Ok(serde_json::json!({ "project_id": project_id, "title": title, "icon_url": icon }))
}

/// Alias aus 0.1.3 -- dort hiess der Befehl so, weil es nur Fabric gab.
#[tauri::command]
pub async fn get_fabric_versions(mc_version: String) -> Result<Vec<String>> {
    let list = crate::loader::versions(crate::loader::LoaderKind::Fabric, &mc_version).await?;
    Ok(list.into_iter().map(|v| v.version).collect())
}

/* ---- Assistent ----------------------------------------------------------- */

#[tauri::command]
pub async fn ai_interpret_search(wish: String) -> Result<Option<crate::ai::SearchIntent>> {
    let config = crate::store::load_config();
    crate::ai::interpret_search(config.model(), &wish).await
}

#[tauri::command]
pub async fn ai_explain_conflict(facts: String) -> Result<Option<crate::ai::ConflictExplanation>> {
    let config = crate::store::load_config();
    crate::ai::explain_conflict(config.model(), &facts).await
}

#[tauri::command]
pub async fn discover_lan(timeout_ms: Option<u64>) -> Vec<crate::server::LanServer> {
    let ms = timeout_ms.unwrap_or(3000).clamp(500, 10_000);
    tokio::task::spawn_blocking(move || crate::server::discover(ms).unwrap_or_default())
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub fn rename_offline_account(account_id: String, username: String) -> Result<Account> {
    crate::auth::offline::validate_username(&username)?;

    crate::store::with_accounts(|accounts| {
        let account = accounts
            .iter_mut()
            .find(|a| a.id == account_id)
            .ok_or_else(|| crate::Error::not_found("Konto nicht gefunden"))?;

        if !account.is_offline() {
            return Err(crate::Error::other(
                "Nur Offline-Konten lassen sich umbenennen — bei Microsoft-Konten \
                 bestimmt Mojang den Namen.",
            ));
        }

        // Die UUID haengt am Namen: sie muss mitwandern, sonst gehoeren die
        // Weltdaten des alten Namens nicht mehr zum Konto.
        account.username = username.trim().to_string();
        account.uuid = crate::auth::offline::offline_uuid(&account.username).to_string();
        Ok(account.clone())
    })
}

/// Was ueber die Anfragegrenzen des gewaehlten Modells bekannt ist.
///
/// Gelernt aus den Antwortkoepfen, nicht fest eingetragen -- Mistral
/// begrenzt nach Tarif und Modell, und die Werte stehen nur im
/// Kontobereich.
#[tauri::command]
pub fn ai_budget(model: Option<String>) -> crate::ai::throttle::Budget {
    let config = crate::store::load_config();
    let name = model.unwrap_or_else(|| config.model().to_string());
    crate::ai::throttle::budget(&name)
}

/* ---- Microsoft-Anmeldung ------------------------------------------------ */

fn ms_client_id() -> Result<String> {
    crate::ai::keys::ms_client_id().ok_or_else(|| {
        crate::Error::other(
            "Es ist keine Azure-Anwendungskennung hinterlegt. Ohne sie kann \
             sich der Launcher nicht bei Microsoft anmelden — Offline-Konten \
             funktionieren weiterhin.",
        )
    })
}

#[tauri::command]
pub fn ms_client_id_status() -> serde_json::Value {
    let id = crate::ai::keys::ms_client_id();
    serde_json::json!({
        "configured": id.is_some(),
        // Nur der Anfang, damit sich zwei Registrierungen unterscheiden
        // lassen, ohne den vollen Wert zu zeigen.
        "hint": id.map(|v| format!("{}…", v.chars().take(8).collect::<String>())),
        "from_env": std::env::var(crate::ai::keys::MS_ENV_VAR).is_ok(),
    })
}

#[tauri::command]
pub fn set_ms_client_id(client_id: Option<String>) -> Result<()> {
    crate::ai::keys::store_ms_client_id(client_id.as_deref())
}

#[tauri::command]
pub async fn start_device_code_login() -> Result<crate::auth::microsoft::DeviceCodeStart> {
    crate::auth::microsoft::start_device_code(&ms_client_id()?).await
}

/// Das Ergebnis einer Abfrage, fuer die Oberflaeche.
///
/// `slow_down` muss getrennt von `pending` ankommen -- sonst fragt die
/// Oberflaeche im selben Abstand weiter und Microsoft drosselt oder lehnt
/// die Anmeldung irgendwann ganz ab, statt nur langsamer zu antworten.
#[derive(serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum DeviceLoginPoll {
    Pending,
    SlowDown,
    Ready { account: Account },
}

/// Fragt einmal nach, ob der Nutzer den Code eingegeben hat.
#[tauri::command]
pub async fn poll_device_code_login(device_code: String) -> Result<DeviceLoginPoll> {
    use crate::auth::microsoft::PollOutcome;

    match crate::auth::microsoft::poll_device_code(&ms_client_id()?, &device_code).await? {
        PollOutcome::Pending => Ok(DeviceLoginPoll::Pending),
        PollOutcome::SlowDown => Ok(DeviceLoginPoll::SlowDown),
        PollOutcome::Ready(account) => {
            let stored = crate::store::with_accounts(|accounts| {
                // Dasselbe Konto nicht zweimal fuehren.
                accounts.retain(|a| a.uuid != account.uuid);
                let mut stored = *account;
                stored.active = accounts.iter().all(|a| !a.active);
                accounts.push(stored.clone());
                Ok(stored)
            })?;
            Ok(DeviceLoginPoll::Ready { account: stored })
        }
    }
}

#[tauri::command]
pub async fn refresh_account(account_id: String) -> Result<Account> {
    let existing = crate::store::load_accounts()
        .into_iter()
        .find(|a| a.id == account_id)
        .ok_or_else(|| crate::Error::not_found("Konto nicht gefunden"))?;

    if existing.is_offline() {
        return Err(crate::Error::other("Offline-Konten laufen nicht ab"));
    }

    let mut fresh =
        crate::auth::microsoft::refresh(&ms_client_id()?, &existing.refresh_token).await?;
    // Kennung und Aktivzustand des bisherigen Eintrags behalten.
    fresh.id = existing.id.clone();
    fresh.active = existing.active;

    // Erst nach dem Netzwerkaufruf sperren und die Liste frisch laden --
    // sonst wuerde ein Stand von vor der Erneuerung zurueckgeschrieben und
    // eine zwischenzeitliche Aenderung (z.B. ein Spielstart, der dasselbe
    // Konto parallel erneuert) waere spurlos weg.
    crate::store::with_accounts(|accounts| {
        for a in accounts.iter_mut() {
            if a.id == account_id {
                *a = fresh.clone();
            }
        }
        Ok(())
    })?;
    Ok(fresh)
}

/* ---- Java besorgen ------------------------------------------------------- */

#[tauri::command]
pub async fn install_java(major: u32) -> Result<JavaInfo> {
    crate::java::install::install(major).await
}

/* ---- Modpacks ------------------------------------------------------------ */

#[tauri::command]
pub fn preview_modpack(path: String) -> Result<crate::modpack::PackPreview> {
    crate::modpack::preview(std::path::Path::new(&path))
}

#[tauri::command]
pub async fn import_modpack(app: AppHandle, path: String) -> Result<InstanceConfig> {
    let (instance, items) = crate::modpack::import(std::path::Path::new(&path))?;
    let config = crate::store::load_config();

    crate::download::fetch_all(
        items,
        config.concurrent_downloads,
        config.verify_sha1,
        "Modpack wird geladen",
        "modpack",
        TauriSink::new(app, "modpack-download-progress"),
    )
    .await?;

    Ok(instance)
}

/// Gibt die Dateinamen zurueck, die sich nicht verweisen liessen --
/// von Hand hinzugefuegte Mods. Sie fehlen im Paket, und das soll der
/// Nutzer wissen, statt es beim Einlesen anderswo zu merken.
#[tauri::command]
pub async fn export_modpack(
    instance_id: String,
    options: crate::modpack::ExportOptions,
    output_path: String,
) -> Result<Vec<String>> {
    crate::modpack::export(&instance_id, &options, std::path::Path::new(&output_path)).await
}

/// Exportiert die Instanz als `.mrpack` in den Downloads-Ordner, damit die Modrinth App sie 1:1 importieren kann
/// (Einstellungen, Server-Liste, Konfiguration, Ressourcen- und Shaderpakete; Welten nicht). Gibt den Pfad zurück.
#[tauri::command]
pub async fn export_for_modrinth(instance_id: String) -> Result<String> {
    let instance = crate::instance::load(&instance_id)?;
    let dir = dirs::download_dir()
        .or_else(dirs::home_dir)
        .ok_or_else(|| crate::Error::other("Der Downloads-Ordner ließ sich nicht bestimmen"))?;
    let safe: String = instance.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let out = dir.join(format!("{safe}.mrpack"));
    let options = crate::modpack::ExportOptions {
        name: instance.name.clone(),
        version: "1.0.0".into(),
        summary: None,
        include_config: true,
        include_packs: true,
        include_worlds: false,
    };
    crate::modpack::export(&instance_id, &options, &out).await?;
    Ok(out.to_string_lossy().into_owned())
}

/* ---- Fremde Launcher ----------------------------------------------------- */

#[tauri::command]
pub fn detect_foreign_launchers() -> Vec<crate::modpack::foreign::ForeignLauncher> {
    crate::modpack::foreign::detect()
}

#[tauri::command]
pub fn import_foreign_instance(path: String) -> Result<InstanceConfig> {
    crate::modpack::foreign::import(std::path::Path::new(&path))
}

/* ---- Selbstaktualisierung ------------------------------------------------ */

#[tauri::command]
pub async fn check_for_update() -> Result<Option<crate::update::UpdateInfo>> {
    let config = crate::store::load_config();
    let found = crate::update::check(env!("CARGO_PKG_VERSION"), false).await?;

    // Eine einmal uebergangene Fassung nicht erneut melden.
    Ok(found.filter(|u| config.skipped_update_version.as_deref() != Some(u.version.as_str())))
}

/// Laedt das Paket herunter und gibt den Pfad zurueck.
///
/// Eingespielt wird bewusst nicht: eine laufende Anwendung durch sich
/// selbst zu ersetzen ist heikel, und ein Paketverwalter will gefragt
/// werden. Der letzte Schritt bleibt beim Nutzer.
#[tauri::command]
pub async fn download_and_install_update(url: String, filename: Option<String>) -> Result<String> {
    let name = filename.unwrap_or_else(|| {
        url.rsplit('/').next().unwrap_or("update").to_string()
    });
    crate::update::download(&url, &name).await
}

/// Anmeldung im Fenster statt über einen Gerätecode.
///
/// Öffnet die Live-Anmeldeseite in einem eigenen Fenster und wartet, bis
/// sie auf die vorgegebene Rückleitung springt -- dort steht der Code in
/// der Adresszeile. Danach schliesst sich das Fenster von selbst.
///
/// Die Eingabe findet in Microsofts Seite statt; der Launcher liest nur
/// die Adresse, nie die Eingabefelder.
#[tauri::command]
pub async fn start_microsoft_login_webview(app: AppHandle) -> Result<Account> {
    use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

    let client_id = ms_client_id()?;
    let url = crate::auth::microsoft::authorize_url(&client_id);
    let parsed = url
        .parse()
        .map_err(|_| crate::Error::other("Die Anmeldeadresse ist ungültig"))?;

    // Ein zurückgebliebenes Fenster aus einem abgebrochenen Versuch
    // würde den neuen Versuch sonst blockieren.
    if let Some(old) = app.get_webview_window("ms-login") {
        let _ = old.close();
    }

    let window = WebviewWindowBuilder::new(&app, "ms-login", WebviewUrl::External(parsed))
        .title("Bei Microsoft anmelden")
        .inner_size(520.0, 720.0)
        .center()
        .build()
        .map_err(|e| crate::Error::other(format!("Anmeldefenster ließ sich nicht öffnen: {e}")))?;

    // Die Adresse abfragen, bis die Rückleitung erscheint. Fünf Minuten
    // reichen auch für Zwei-Faktor-Anmeldung und Passwortsuche.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(300);
    let code = loop {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;

        // Hat der Nutzer das Fenster geschlossen, ist das ein Abbruch.
        let Some(current) = app.get_webview_window("ms-login") else {
            return Err(crate::Error::other("Die Anmeldung wurde abgebrochen."));
        };

        if let Ok(here) = current.url() {
            let text = here.as_str();
            if text.starts_with("https://login.live.com/oauth20_desktop.srf") {
                let result = crate::auth::microsoft::code_from_redirect(text);
                let _ = current.close();
                break result?;
            }
        }

        if std::time::Instant::now() > deadline {
            let _ = current.close();
            return Err(crate::Error::other(
                "Die Anmeldung hat zu lange gedauert. Bitte erneut versuchen.",
            ));
        }
    };

    let _ = window.close();

    let account = crate::auth::microsoft::exchange_code(&client_id, &code).await?;
    let account = crate::store::with_accounts(|accounts| {
        accounts.retain(|a| a.uuid != account.uuid);
        let mut account = account;
        account.active = accounts.iter().all(|a| !a.active);
        accounts.push(account.clone());
        Ok(account)
    })?;

    Ok(account)
}

/* ---- Instanz-Assistent --------------------------------------------------- */

use crate::assistant::{brain, plan as plan_mod, questions, search, session};

/// Baut den Zusammenhang, den der Katalog braucht.
async fn assistant_context() -> questions::Context {
    let versions = crate::manifest::list_versions()
        .await
        .map(|v| {
            v.into_iter()
                .filter(|e| matches!(e.channel, crate::manifest::VersionChannel::Release))
                .map(|e| e.id)
                .collect()
        })
        .unwrap_or_default();

    questions::Context { versions, system_ram_mb: get_system_memory() as u32 }
}

/// Beginnt eine Sitzung.
///
/// Mit Schlüssel wird der Katalog auf die Beschreibung zugeschnitten; ohne
/// ihn -- oder wenn das Modell ausfällt -- kommt der Standardkatalog. Der
/// Fehler wird dabei durchgereicht, damit die Oberfläche fragen kann, ob
/// mit dem Ersatzmodell weitergemacht werden soll.
/// `research`: weglassen uebernimmt die zuletzt gewaehlte Voreinstellung.
/// Explizit gesetzt, wird der Wert zugleich zur neuen Voreinstellung -- die
/// Wahl im Assistenten gilt fuer kuenftige Laeufe, bis sie geaendert wird,
/// genau wie beim Modell.
#[tauri::command]
pub async fn assistant_start(
    app: AppHandle,
    goal: String,
    model: Option<String>,
    research: Option<bool>,
) -> Result<session::Session> {
    let mut config = crate::store::load_config();
    let ctx = assistant_context().await;
    let chosen = model.unwrap_or_else(|| config.model().to_string());
    let research = research.unwrap_or(config.assistant_web_research);
    if research != config.assistant_web_research {
        config.assistant_web_research = research;
        crate::store::save_config(&config)?;
    }

    let mut s = session::Session::new(&goal);
    let ai_events = crate::bridge::TauriAiEvents::new(app, s.id.clone());

    match brain::tailor_catalogue(&chosen, &goal, &ctx, research, &ai_events).await {
        Ok(Some((tailored, reading))) => {
            s.questions = tailored;
            s.source = "modell".into();
            if !reading.trim().is_empty() {
                s.notes.push(reading);
            }
        }
        Ok(None) => {
            // Kein Schlüssel -- der geführte Assistent.
            s.questions = questions::catalogue(&ctx);
            s.source = "katalog".into();
        }
        Err(e) if e.is_model_unavailable() => {
            // Durchreichen: die Oberfläche fragt nach dem Ersatzmodell.
            return Err(e);
        }
        Err(e) => {
            eprintln!("[assistent] Katalog konnte nicht zugeschnitten werden: {e}");
            s.questions = questions::catalogue(&ctx);
            s.source = "katalog".into();
            s.notes.push(
                "Der Fragebogen konnte nicht auf deine Beschreibung zugeschnitten \
                 werden — es gilt der Standardkatalog."
                    .into(),
            );
        }
    }

    session::save(&s)?;
    Ok(s)
}

#[tauri::command]
pub fn assistant_load(session_id: String) -> Result<session::Session> {
    session::load(&session_id)
}

#[tauri::command]
pub fn assistant_open_sessions() -> Vec<session::Session> {
    session::list_open()
}

#[tauri::command]
pub fn assistant_discard(session_id: String) -> Result<()> {
    session::delete(&session_id)
}

/// Nimmt eine Antwort entgegen.
#[tauri::command]
pub fn assistant_answer(
    session_id: String,
    question_id: String,
    choices: Vec<String>,
) -> Result<session::Session> {
    let mut s = session::load(&session_id)?;
    s.answer(&question_id, choices)?;
    if s.complete() {
        s.step = session::Step::Choose;
    }
    session::save(&s)?;
    Ok(s)
}

/// Sucht die Vorschläge zu den Antworten -- ohne Modell.
#[tauri::command]
pub async fn assistant_candidates(session_id: String) -> Result<session::Session> {
    let mut s = session::load(&session_id)?;
    let outcome = s.outcome();

    // Ein Fehlschlag darf nicht als "nichts gefunden" durchgehen -- sonst
    // sieht der Nutzer eine leere Auswahl und erfaehrt nie, warum.
    s.candidates = match search::candidates(
        &outcome.searches,
        outcome.mc_version.as_deref(),
        outcome.loader.as_deref(),
    )
    .await
    {
        Ok(found) => found,
        Err(e) => {
            s.searched = true;
            s.notes.push(format!("Die Suche schlug fehl: {e}"));
            session::save(&s)?;
            return Err(e);
        }
    };
    s.searched = true;

    if s.candidates.is_empty() {
        s.notes.push(
            "Zu diesen Antworten wurde nichts gefunden, das zu Version und Lader \
             passt. Eine andere Minecraft-Version ist meist der schnellste Weg."
                .into(),
        );
    }

    // Was mehrfach getroffen wurde, ist vorausgewählt.
    s.selected = s
        .candidates
        .iter()
        .filter(|c| c.preselected)
        .map(|c| c.project.project_id.clone())
        .collect();

    s.notes.extend(outcome.notes);
    session::save(&s)?;
    Ok(s)
}

#[tauri::command]
pub fn assistant_select(session_id: String, project_ids: Vec<String>) -> Result<session::Session> {
    let mut s = session::load(&session_id)?;
    s.selected = project_ids;
    session::save(&s)?;
    Ok(s)
}

/// Stellt den Plan zusammen.
///
/// Der Plan entsteht ohne Modell; ist ein Schlüssel hinterlegt, begründet
/// es ihn anschliessend. Fällt das aus, fehlen die Begründungen -- nicht
/// der Plan.
#[tauri::command]
pub async fn assistant_plan(
    app: AppHandle,
    session_id: String,
    name: Option<String>,
    model: Option<String>,
) -> Result<session::Session> {
    let mut s = session::load(&session_id)?;
    let config = crate::store::load_config();
    let chosen = model.unwrap_or_else(|| config.model().to_string());
    let ai_events = crate::bridge::TauriAiEvents::new(app, s.id.clone());

    let mut built = plan_mod::build(&s, name.as_deref().unwrap_or("")).await?;

    match brain::explain_plan(&chosen, &s, &mut built, &ai_events).await {
        Ok(_) => {}
        Err(e) if e.is_model_unavailable() => {
            // Plan behalten, damit die Arbeit nicht verloren geht.
            s.plan = Some(built);
            s.step = session::Step::Review;
            session::save(&s)?;
            return Err(e);
        }
        Err(e) => eprintln!("[assistent] Begründung fehlgeschlagen: {e}"),
    }

    s.plan = Some(built);
    s.step = session::Step::Review;
    session::save(&s)?;
    Ok(s)
}

/// Setzt den bestätigten Plan um.
#[tauri::command]
pub async fn assistant_apply(
    app: AppHandle,
    session_id: String,
) -> Result<InstanceConfig> {
    let mut s = session::load(&session_id)?;
    let plan = s
        .plan
        .clone()
        .ok_or_else(|| crate::Error::other("Es liegt noch kein Plan vor"))?;

    let instance =
        plan_mod::apply(&plan, TauriSink::new(app, "download-progress")).await?;

    // Die Beschreibung bleibt an der Instanz -- sie sagt später, wofür sie
    // gedacht war.
    if !s.goal.trim().is_empty() {
        let mut with_notes = crate::instance::load(&instance.id)?;
        with_notes.notes = s.goal.clone();
        crate::instance::save(&with_notes)?;
    }

    s.step = session::Step::Done;
    session::save(&s)?;
    Ok(instance)
}

/* ---- Bedrock ------------------------------------------------------------ */

#[tauri::command]
pub fn bedrock_status() -> Result<crate::bedrock::detect::BedrockStatus> {
    crate::bedrock::detect::status()
}

/// Oeffnet `mcpelauncher-ui-qt` fuer Google-Anmeldung/Versions-Download --
/// ein vollwertiges, dauerhaftes Fenster, kein Einmal-Dialog. BuildMC wartet
/// bewusst nicht auf dessen Ende; der Nutzer druckt hinterher selbst auf
/// "Versionen neu einlesen".
#[tauri::command]
pub fn bedrock_open_ui() -> Result<()> {
    let launcher = crate::bedrock::detect::find_ui().ok_or_else(|| {
        crate::Error::other("mcpelauncher-ui-qt wurde nicht gefunden")
    })?;
    launcher.command("mcpelauncher-ui-qt")
        .spawn()
        .map_err(|e| crate::Error::other(format!("mcpelauncher-ui-qt ließ sich nicht starten: {e}")))?;
    Ok(())
}

#[tauri::command]
pub fn bedrock_list_versions() -> Result<Vec<crate::bedrock::detect::BedrockVersion>> {
    crate::bedrock::detect::list_versions()
}

#[tauri::command]
pub fn bedrock_list_instances() -> Result<Vec<crate::bedrock::BedrockInstance>> {
    crate::bedrock::list()
}

#[tauri::command]
pub fn bedrock_create_instance(name: String, version_dir_name: String) -> Result<crate::bedrock::BedrockInstance> {
    crate::bedrock::create(&name, &version_dir_name)
}

#[tauri::command]
pub fn bedrock_start(app: AppHandle, instance_id: String) -> Result<crate::bedrock::launch::RunningBedrockInstance> {
    let client = crate::bedrock::detect::find_client().ok_or_else(|| {
        crate::Error::other("mcpelauncher-client wurde nicht gefunden")
    })?;
    let instance = crate::bedrock::load(&instance_id)?;
    let game_dir = instance.game_dir()?;
    let data_dir = instance.data_dir()?;
    let cache_dir = instance.cache_dir()?;

    let running = crate::bedrock::launch::start(
        &instance_id,
        &client,
        &game_dir,
        &data_dir,
        &cache_dir,
        TauriBedrockEvents::new(app),
    )?;

    let mut instance = instance;
    instance.last_played = crate::bedrock::now();
    instance.launch_count += 1;
    crate::bedrock::save(&instance)?;

    Ok(running)
}

#[tauri::command]
pub fn bedrock_stop(instance_id: String) -> Result<()> {
    crate::bedrock::launch::stop(&instance_id)
}

#[tauri::command]
pub fn bedrock_running() -> Vec<crate::bedrock::launch::RunningBedrockInstance> {
    crate::bedrock::launch::running()
}

/* ---- Rust-Editionen: Horizon (Option 3) und BHH (Option 4) ---------------- */

fn variante(v: &Option<String>) -> crate::rust_edition::Variant {
    crate::rust_edition::Variant::parse(v.as_deref())
}

#[tauri::command]
pub fn rust_status(variant: Option<String>) -> Result<crate::rust_edition::engine::RustStatus> {
    crate::rust_edition::engine::status(variante(&variant))
}

/// Baut die Variante aus ihrem Quellcode. Laeuft im Hintergrund; jede Zeile
/// geht als `rust-build` hinaus (mit `variant`), das Ende mit `done`.
#[tauri::command]
pub async fn rust_build(app: AppHandle, variant: Option<String>) -> Result<()> {
    use tauri::Emitter;
    let v = variante(&variant);
    let app2 = app.clone();
    let ergebnis = tauri::async_runtime::spawn_blocking(move || {
        crate::rust_edition::engine::build(v, |zeile, anteil| {
            let _ = app2.emit("rust-build", serde_json::json!({ "variant": v, "line": zeile, "fraction": anteil, "done": false }));
        })
    })
    .await
    .map_err(|e| crate::Error::other(format!("Bau abgebrochen: {e}")))?;
    let _ = app.emit("rust-build", serde_json::json!({
        "variant": v, "line": "", "fraction": 1.0, "done": true,
        "error": ergebnis.as_ref().err().map(|e| e.to_string()),
    }));
    ergebnis
}

#[tauri::command]
pub async fn rust_prepare(app: AppHandle, variant: Option<String>) -> Result<()> {
    crate::rust_edition::prepare::prepare(variante(&variant), TauriSink::new(app, "rust-prepare-progress")).await
}

#[tauri::command]
pub fn rust_list_instances(variant: Option<String>) -> Result<Vec<crate::rust_edition::RustInstance>> {
    crate::rust_edition::list(variante(&variant))
}

#[tauri::command]
pub fn rust_create_instance(variant: Option<String>, name: String, quick_server: Option<String>) -> Result<crate::rust_edition::RustInstance> {
    crate::rust_edition::create(variante(&variant), &name, quick_server.as_deref().unwrap_or(""))
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn rust_update_instance(
    variant: Option<String>,
    instance_id: String,
    name: Option<String>,
    quick_server: Option<String>,
    quick_world: Option<String>,
    fullscreen: Option<bool>,
    jvm_heap_mb: Option<u32>,
) -> Result<crate::rust_edition::RustInstance> {
    crate::rust_edition::update(variante(&variant), &instance_id, crate::rust_edition::Aenderung {
        name: name.as_deref(),
        quick_server: quick_server.as_deref(),
        quick_world: quick_world.as_deref(),
        fullscreen,
        jvm_heap_mb,
    })
}

#[tauri::command]
pub fn rust_delete_instance(variant: Option<String>, instance_id: String) -> Result<()> {
    crate::rust_edition::delete(variante(&variant), &instance_id)
}

#[tauri::command]
pub async fn rust_start(app: AppHandle, variant: Option<String>, instance_id: String) -> Result<crate::rust_edition::launch::RunningRustInstance> {
    crate::rust_edition::launch::start(variante(&variant), &instance_id, TauriRustEvents::new(app)).await
}

#[tauri::command]
pub fn rust_stop(instance_id: String) -> Result<()> {
    crate::rust_edition::launch::stop(&instance_id)
}

#[tauri::command]
pub fn rust_running() -> Vec<crate::rust_edition::launch::RunningRustInstance> {
    crate::rust_edition::launch::running()
}

/* ---- BHH: Fabric-Mods ---------------------------------------------------- */

#[tauri::command]
pub fn bhh_list_mods(instance_id: String) -> Result<Vec<crate::rust_edition::fabric::FabricMod>> {
    crate::rust_edition::fabric::list(&instance_id)
}

#[tauri::command]
pub fn bhh_add_mod(instance_id: String, path: String) -> Result<crate::rust_edition::fabric::FabricMod> {
    crate::rust_edition::fabric::add(&instance_id, std::path::Path::new(&path))
}

/// Dateiauswahl (nur .jar) und alle gewaehlten Mods in die Instanz kopieren.
#[tauri::command]
pub async fn bhh_pick_mods(app: AppHandle, instance_id: String) -> Result<Vec<crate::rust_edition::fabric::FabricMod>> {
    use tauri_plugin_dialog::DialogExt;
    let gewaehlt = tauri::async_runtime::spawn_blocking(move || {
        app.dialog().file().add_filter("Fabric-Mod", &["jar"]).blocking_pick_files()
    }).await.map_err(|e| crate::Error::other(e.to_string()))?.unwrap_or_default();
    let mut out = Vec::new();
    for f in gewaehlt {
        let pfad = f.into_path().map_err(|e| crate::Error::other(e.to_string()))?;
        out.push(crate::rust_edition::fabric::add(&instance_id, &pfad)?);
    }
    Ok(out)
}

#[tauri::command]
pub fn bhh_remove_mod(instance_id: String, file: String) -> Result<()> {
    crate::rust_edition::fabric::remove(&instance_id, &file)
}

/// Den Mod-Ordner der Instanz im Dateimanager oeffnen (Mods hineinziehen).
#[tauri::command]
pub fn bhh_open_mods_dir(instance_id: String) -> Result<()> {
    let i = crate::rust_edition::load(crate::rust_edition::Variant::Bhh, &instance_id)?;
    let dir = i.fabric_mods_dir()?;
    crate::paths::ensure_dir(&dir)?;
    open::that(&dir).map_err(|e| crate::Error::other(format!("Ordner ließ sich nicht öffnen: {e}")))
}
