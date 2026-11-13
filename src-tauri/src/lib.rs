//! BuildMC Launcher -- Backend.

pub mod ai;
pub mod assistant;
pub mod auth;
pub mod bedrock;
pub mod bridge;
pub mod commands;
pub mod curseforge;
pub mod download;
pub mod error;
pub mod http;
pub mod instance;
pub mod java;
pub mod launch;
pub mod loader;
pub mod manifest;
pub mod modpack;
pub mod modrinth;
pub mod mods;
pub mod news;
pub mod optimize;
pub mod packs;
pub mod paths;
pub mod rust_edition;
pub mod security;
pub mod serde_util;
pub mod server;
pub mod snapshot;
pub mod store;
pub mod update;

pub use error::{Error, Result};

/// Startet die Anwendung.
pub fn run() {
    // Die Verzeichnisse einmal anlegen, damit spaetere Schreibzugriffe
    // nicht einzeln daran scheitern.
    if let Err(e) = prepare_dirs() {
        eprintln!("[start] Datenverzeichnis ließ sich nicht anlegen: {e}");
    }

    tauri::Builder::default()
        .setup(|app| {
            // Live-Werte senden, solange die Anwendung laeuft.
            bridge::start_telemetry(app.handle().clone());
            // Mitgelieferte Rust-Editionen (Horizon, BHH) liegen in den Anwendungs-Ressourcen.
            {
                use tauri::Manager;
                if let Ok(dir) = app.path().resource_dir() {
                    rust_edition::engine::set_resource_dir(dir.join("rust-editions"));
                }
            }
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            // Einstellungen
            commands::get_config,
            commands::save_config,
            commands::get_system_memory,
            // Instanzen
            commands::list_instances,
            commands::create_instance,
            commands::update_instance,
            commands::open_instance_folder,
            commands::open_external,
            // Versionen und Java
            commands::get_versions,
            commands::download_version,
            commands::detect_java,
            commands::check_java_path,
            // Starten
            commands::launch_instance,
            commands::launch_in_official_launcher,
            commands::stop_instance,
            commands::get_running_instances,
            commands::get_launch_log,
            commands::clear_launch_log,
            commands::analyze_log,
            commands::analyze_log_text,
            commands::suggest_jvm_flags,
            commands::list_worlds,
            // Konten
            commands::get_accounts,
            commands::add_offline_account,
            commands::set_active_account,
            commands::remove_account,
            commands::rename_offline_account,
            commands::ms_client_id_status,
            commands::set_ms_client_id,
            commands::start_device_code_login,
            commands::start_microsoft_login_webview,
            commands::poll_device_code_login,
            commands::refresh_account,
            // Server
            commands::list_servers,
            commands::save_server,
            commands::delete_server,
            // Instanzverwaltung
            commands::trash_instance,
            commands::restore_instance,
            commands::permanently_delete_instance,
            commands::list_trashed_instances,
            commands::duplicate_instance,
            commands::set_instance_group,
            // Server-Status
            commands::ping_server,
            commands::discover_lan,
            // Sicherungen
            commands::list_snapshots,
            commands::create_snapshot,
            commands::restore_snapshot,
            commands::delete_snapshot,
            // Laufzeit
            commands::get_telemetry,
            commands::list_crash_reports,
            commands::list_installed_java,
            commands::install_java,
            // Modpacks
            commands::preview_modpack,
            commands::import_modpack,
            commands::export_modpack,
            // Fremde Launcher
            commands::detect_foreign_launchers,
            commands::import_foreign_instance,
            // Selbstaktualisierung
            commands::check_for_update,
            commands::download_and_install_update,
            // Modrinth
            commands::get_project_info,
            commands::get_fabric_versions,
            // Loader
            commands::get_loader_versions,
            commands::install_loader,
            // Mods
            commands::search_mods,
            commands::get_mod_versions,
            commands::list_instance_mods,
            commands::install_mod,
            commands::install_shader_pack,
            commands::remove_mod,
            commands::toggle_mod,
            commands::add_local_mod,
            commands::check_mod_updates,
            commands::update_all_mods,
            commands::resolve_mod_install,
            commands::apply_mod_plan,
            // Instanz-Assistent
            commands::assistant_start,
            commands::assistant_load,
            commands::assistant_open_sessions,
            commands::assistant_discard,
            commands::assistant_answer,
            commands::assistant_candidates,
            commands::assistant_select,
            commands::assistant_plan,
            commands::assistant_apply,
            // Assistent
            commands::ai_status,
            commands::ai_set_key,
            commands::ai_set_model,
            commands::curseforge_status,
            commands::curseforge_set_key,
            commands::security_status,
            commands::security_set_key,
            commands::security_set_mode,
            commands::security_set_auto_scan,
            commands::security_scan_instance,
            commands::security_history,
            commands::security_allow_hash,
            commands::security_upload_unknown,
            commands::ai_list_models,
            commands::ai_budget,
            commands::ai_interpret_search,
            commands::ai_explain_conflict,
            // Bedrock
            commands::bedrock_status,
            commands::bedrock_open_ui,
            commands::bedrock_list_versions,
            commands::bedrock_list_instances,
            commands::bedrock_create_instance,
            commands::bedrock_start,
            commands::bedrock_stop,
            commands::bedrock_running,
            // Rust-Edition (Horizon)
            commands::rust_status,
            commands::rust_build,
            commands::rust_prepare,
            commands::rust_list_instances,
            commands::rust_create_instance,
            commands::rust_update_instance,
            commands::rust_delete_instance,
            commands::rust_start,
            commands::rust_stop,
            commands::rust_running,
            commands::bhh_list_mods,
            commands::bhh_add_mod,
            commands::bhh_pick_mods,
            commands::bhh_remove_mod,
            commands::bhh_open_mods_dir,
        ])
        .run(tauri::generate_context!())
        .expect("Die Anwendung ließ sich nicht starten");
}

fn prepare_dirs() -> Result<()> {
    for dir in [
        paths::root()?,
        paths::instances()?,
        paths::versions()?,
        paths::libraries()?,
        paths::assets()?,
        paths::natives()?,
        paths::logs()?,
    ] {
        paths::ensure_dir(&dir)?;
    }
    Ok(())
}
