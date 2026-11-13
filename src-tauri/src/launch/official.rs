//! Übergabe an den offiziellen Minecraft-Launcher.
//!
//! Statt das Spiel selbst zu starten (dafür wäre eine eigene, von Mojang freigegebene Anwendungs-ID
//! nötig), legt der Launcher die Instanz als Profil im offiziellen Launcher an: die Fabric-Version
//! unter `.minecraft/versions/`, ein Profil in `launcher_profiles.json` mit dem Instanzordner als
//! Spielordner (dort liegen die Mods), und öffnet danach den offiziellen Launcher. Anmeldung und
//! Spielstart macht dieser; der Nutzer drückt dort auf „Spielen“. So arbeitet auch der Fabric-Installer.

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::instance::InstanceConfig;
use crate::loader::{self, LoaderKind};
use crate::{Error, Result};

/// Das Datenverzeichnis des offiziellen Minecraft-Launchers.
pub fn minecraft_dir() -> Result<PathBuf> {
    let no_home = || Error::other("Das Benutzerverzeichnis ließ sich nicht bestimmen");
    if cfg!(windows) {
        Ok(dirs::data_dir().ok_or_else(no_home)?.join(".minecraft"))
    } else if cfg!(target_os = "macos") {
        Ok(dirs::data_dir().ok_or_else(no_home)?.join("minecraft"))
    } else {
        let home = dirs::home_dir().ok_or_else(no_home)?;
        let flatpak = home.join(".var/app/com.mojang.Minecraft/.minecraft");
        Ok(if flatpak.exists() { flatpak } else { home.join(".minecraft") })
    }
}

/// Legt Version und Profil an und gibt die Versions-ID zurück.
pub async fn install_profile(instance: &InstanceConfig) -> Result<String> {
    let mc = minecraft_dir()?;
    if !mc.exists() {
        return Err(Error::other(
            "Es gibt noch keinen .minecraft-Ordner. Starte den offiziellen Minecraft-Launcher \
             einmal und melde dich dort an.",
        ));
    }

    // 1. die Version: Fabric/Quilt-Profil in den Ordner des offiziellen Launchers kopieren
    let version_id = match LoaderKind::parse(&instance.loader).or_else(|| (instance.loader == "buildmc").then_some(LoaderKind::Fabric)) {
        Some(kind) if matches!(kind, LoaderKind::Fabric | LoaderKind::Quilt) => {
            let loader_version = match instance.fabric_version.clone() {
                Some(v) if !v.is_empty() => v,
                _ => loader::recommended(kind, &instance.mc_version).await?,
            };
            let id = loader::install(kind, &instance.mc_version, &loader_version).await?;
            let src = crate::paths::version_json(&id)?;
            let dst = mc.join("versions").join(&id).join(format!("{id}.json"));
            std::fs::create_dir_all(dst.parent().expect("version dir"))?;
            std::fs::copy(&src, &dst)?;
            id
        }
        Some(kind) => return Err(Error::other(format!("{} wird für die Übergabe noch nicht unterstützt.", kind.label()))),
        None => instance.mc_version.clone(), // Vanilla: die Version bringt der offizielle Launcher selbst mit
    };

    // 2. das Profil in launcher_profiles.json
    let game_dir = crate::launch::ensure_game_dir(instance)?;
    let path = mc.join("launcher_profiles.json");
    let mut doc: Value = if path.exists() {
        let raw = std::fs::read_to_string(&path)?;
        let _ = std::fs::write(mc.join("launcher_profiles.json.buildmc-backup"), &raw);
        serde_json::from_str(&raw).map_err(|_| Error::other("launcher_profiles.json ist nicht lesbar; es wurde nichts verändert."))?
    } else {
        json!({ "profiles": {}, "settings": {}, "version": 3 })
    };
    let now = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
    let mut java_args = format!("-Xmx{}M -Xms{}M", instance.max_ram, instance.min_ram.min(instance.max_ram));
    for a in &instance.jvm_args {
        java_args.push(' ');
        java_args.push_str(a);
    }
    let profiles = doc
        .as_object_mut()
        .ok_or_else(|| Error::other("launcher_profiles.json hat ein unerwartetes Format; es wurde nichts verändert."))?
        .entry("profiles")
        .or_insert_with(|| json!({}));
    let key = format!("buildmc-{}", instance.id);
    let created = profiles.get(&key).and_then(|p| p.get("created")).cloned().unwrap_or_else(|| json!(now));
    profiles[key] = json!({
        "created": created, "lastUsed": now, "icon": "Furnace", "type": "custom",
        "name": format!("BuildMC – {}", instance.name), "lastVersionId": version_id,
        "gameDir": game_dir.to_string_lossy(), "javaArgs": java_args,
    });
    std::fs::write(&path, serde_json::to_string_pretty(&doc).map_err(|e| Error::other(e.to_string()))?)?;
    Ok(version_id)
}

/// Öffnet den offiziellen Launcher, soweit er sich finden lässt. `false`: nicht gefunden oder nicht installiert.
pub fn open_launcher() -> bool {
    use std::process::{Command, Stdio};
    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Ok(base) = std::env::var(var) {
                let exe = PathBuf::from(base).join("Minecraft Launcher").join("MinecraftLauncher.exe");
                if exe.exists() && Command::new(&exe).spawn().is_ok() {
                    return true;
                }
            }
        }
        // Microsoft-Store-Version: nur starten, wenn das Paket wirklich installiert ist
        let installed = dirs::data_local_dir()
            .map(|d| d.join("Packages").join("Microsoft.4297127D64EC6_8wekyb3d8bbwe").exists())
            .unwrap_or(false);
        return installed
            && Command::new("explorer.exe").arg("shell:AppsFolder\\Microsoft.4297127D64EC6_8wekyb3d8bbwe!Minecraft").spawn().is_ok();
    }
    // Linux: erst das normale Programm (fehlt es, scheitert der Start sofort), dann Flatpak nur, wenn es installiert ist
    if Command::new("minecraft-launcher").spawn().is_ok() {
        return true;
    }
    let flatpak_installed = Command::new("flatpak").args(["info", "com.mojang.Minecraft"]).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    flatpak_installed && Command::new("flatpak").args(["run", "com.mojang.Minecraft"]).spawn().is_ok()
}
