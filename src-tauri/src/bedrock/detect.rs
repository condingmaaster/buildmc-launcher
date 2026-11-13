//! Erkennung von `mcpelauncher` und den bereits heruntergeladenen
//! Bedrock-Versionen.
//!
//! Schritt 1 (dieser Plan): nur erkennen, ob eine Installation vorhanden
//! ist -- PATH, AppImage, Flatpak. Kein Mitliefern, siehe die Begruendung
//! im Plan ("Buendelung"). Reine Pfad-Existenzpruefung, kein
//! `--version`-Unterprozess bei jedem Status-Abruf.

use std::path::PathBuf;

use serde::Serialize;

use crate::error::Result;

#[derive(Debug, Clone, Serialize)]
pub struct BedrockStatus {
    /// `mcpelauncher` deckt kein Windows ab -- dort gibt es Bedrock
    /// offiziell ueber den Microsoft Store. Auf Windows-Builds ist das
    /// hier `false`, unabhaengig davon, ob irgendetwas gefunden wurde --
    /// die Oberflaeche zeigt dann "nicht verfuegbar", nicht "nicht
    /// gefunden".
    pub platform_supported: bool,
    pub client_found: bool,
    pub client_path: Option<String>,
    pub ui_found: bool,
    pub ui_path: Option<String>,
    /// Reine Existenzpruefung von `[googlelogin]/identifier` in
    /// `mcpelauncher-ui-qt`s eigener Konfigurationsdatei -- keine
    /// Entschluesselung, kein Zugriff auf das eigentliche Google-Token.
    pub google_account_linked: bool,
    pub available_versions: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct BedrockVersion {
    pub dir_name: String,
}

/// Wo ein mcpelauncher-Programm gefunden wurde -- ein eigenstaendiges
/// Binary im PATH, oder das Flatpak-Paket. Beides braucht einen anderen
/// Aufruf: ein Binary wird direkt gestartet, Flatpak nur ueber
/// `flatpak run --command=... <App-Kennung>`.
#[derive(Debug, Clone)]
pub enum Launcher {
    Binary(PathBuf),
    Flatpak,
}

impl Launcher {
    fn display(&self) -> String {
        match self {
            Launcher::Binary(p) => p.display().to_string(),
            Launcher::Flatpak => "Flatpak (io.mrarm.mcpelauncher)".to_string(),
        }
    }

    /// Baut den fertigen Aufruf fuer das benannte Programm innerhalb des
    /// Pakets (`mcpelauncher-client` oder `mcpelauncher-ui-qt`).
    pub fn command(&self, program: &str) -> std::process::Command {
        match self {
            Launcher::Binary(p) => std::process::Command::new(p),
            Launcher::Flatpak => {
                let mut c = std::process::Command::new("flatpak");
                c.args(["run", &format!("--command={program}"), "io.mrarm.mcpelauncher"]);
                c
            }
        }
    }
}

/// Ist `io.mrarm.mcpelauncher` als Flatpak installiert?
///
/// **Nicht** ueber `~/.var/app/<id>` pruefen -- das ist das
/// Laufzeit-Datenverzeichnis der App und entsteht erst beim ersten Start,
/// nicht bei der Installation (an einer echten Installation nachvollzogen).
/// Die tatsaechliche Installation steht unter
/// `/var/lib/flatpak/app/<id>` (systemweit) oder
/// `~/.local/share/flatpak/app/<id>` (je Nutzer).
fn flatpak_installed() -> bool {
    const ID: &str = "io.mrarm.mcpelauncher";
    if PathBuf::from("/var/lib/flatpak/app").join(ID).exists() {
        return true;
    }
    if let Some(home) = dirs::home_dir() {
        if home.join(".local/share/flatpak/app").join(ID).exists() {
            return true;
        }
    }
    false
}

/// Sucht `mcpelauncher-client` -- zuerst im PATH, dann als Flatpak-Paket.
/// Reine Pfad-/Installationspruefung, kein Prozessstart.
pub fn find_client() -> Option<Launcher> {
    if let Ok(path) = which("mcpelauncher-client") {
        return Some(Launcher::Binary(path));
    }
    flatpak_installed().then_some(Launcher::Flatpak)
}

pub fn find_ui() -> Option<Launcher> {
    if let Ok(path) = which("mcpelauncher-ui-qt") {
        return Some(Launcher::Binary(path));
    }
    flatpak_installed().then_some(Launcher::Flatpak)
}

/// Bare-Bones-PATH-Suche ohne externe Abhaengigkeit -- kein
/// `--version`-Aufruf wie bei `security::clamav::programm_da`, das dort
/// noetig ist, weil eine evtl. selbst installierte Version erkannt werden
/// muss; hier reicht die reine Existenz, kein Grund einen Prozess zu
/// starten.
fn which(name: &str) -> std::result::Result<PathBuf, ()> {
    let path = std::env::var_os("PATH").ok_or(())?;
    for dir in std::env::split_paths(&path) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(())
}

/// `$XDG_DATA_HOME/mcpelauncher/versions/` -- wo `mcpelauncher-ui-qt`
/// heruntergeladene Bedrock-Versionen ablegt. Belegt im echten Quelltext
/// von `profilemanager.cpp` (`qt6`-Branch).
pub fn versions_dir() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| crate::Error::other("Datenverzeichnis des Systems nicht ermittelbar"))?;
    Ok(base.join("mcpelauncher").join("versions"))
}

/// `~/.config/Minecraft Linux Launcher/Minecraft Linux Launcher UI.conf` --
/// Qt legt seine native `QSettings`-Datei unter Organisation/Anwendungsname
/// ab (`main.cpp`: `setOrganizationName`/`setApplicationName`). Direkt im
/// Quelltext verifiziert, nicht angenommen.
fn google_login_config_file() -> Option<PathBuf> {
    dirs::config_dir().map(|c| {
        c.join("Minecraft Linux Launcher")
            .join("Minecraft Linux Launcher UI.conf")
    })
}

/// Reine Existenzpruefung von `[googlelogin]/identifier` -- kein
/// Entschluesseln, kein Zugriff auf das eigentliche Token. Siehe Plan fuer
/// die Begruendung, warum eine volle Entschluesselung bewusst nicht gebaut
/// wird.
pub fn google_account_linked() -> bool {
    let Some(path) = google_login_config_file() else { return false };
    let Ok(text) = std::fs::read_to_string(&path) else { return false };

    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            in_group = line == "[googlelogin]";
            continue;
        }
        if in_group && line.starts_with("identifier") && line.contains('=') {
            return true;
        }
    }
    false
}

/// Listet die bereits heruntergeladenen Bedrock-Versionen.
pub fn list_versions() -> Result<Vec<BedrockVersion>> {
    let dir = versions_dir()?;
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let Ok(entry) = entry else { continue };
        if !entry.path().is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        out.push(BedrockVersion { dir_name: name });
    }
    out.sort_by(|a, b| b.dir_name.cmp(&a.dir_name));
    Ok(out)
}

pub fn status() -> Result<BedrockStatus> {
    let client = find_client();
    let ui = find_ui();
    let versions = list_versions()?;

    Ok(BedrockStatus {
        platform_supported: !cfg!(target_os = "windows"),
        client_found: client.is_some(),
        client_path: client.map(|l| l.display()),
        ui_found: ui.is_some(),
        ui_path: ui.map(|l| l.display()),
        google_account_linked: google_account_linked(),
        available_versions: versions.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_login_gruppe_wird_erkannt() {
        let text = "[General]\nkey=abc\n\n[googlelogin]\nencrypted=xyz\nidentifier=abc123\n";
        let mut in_group = false;
        let mut found = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_group = line == "[googlelogin]";
                continue;
            }
            if in_group && line.starts_with("identifier") && line.contains('=') {
                found = true;
            }
        }
        assert!(found);
    }

    #[test]
    fn andere_gruppe_zaehlt_nicht() {
        let text = "[General]\nidentifier=nicht-das-hier\n";
        let mut in_group = false;
        let mut found = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') && line.ends_with(']') {
                in_group = line == "[googlelogin]";
                continue;
            }
            if in_group && line.starts_with("identifier") && line.contains('=') {
                found = true;
            }
        }
        assert!(!found, "identifier ausserhalb von [googlelogin] darf nicht zaehlen");
    }
}
