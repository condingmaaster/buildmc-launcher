//! Wo alles liegt.
//!
//! Das Verzeichnis ist absichtlich identisch zu Version 0.1.3
//! (`~/.local/share/buildmc-launcher/` bzw. `%APPDATA%\buildmc-launcher\`),
//! damit eine bestehende Installation ohne Migration weiterlaeuft.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

const DIR_NAME: &str = "buildmc-launcher";

/// Wurzelverzeichnis aller Launcher-Daten.
pub fn root() -> Result<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| Error::other("Datenverzeichnis des Systems nicht ermittelbar"))?;
    Ok(base.join(DIR_NAME))
}

macro_rules! sub {
    ($(#[$doc:meta] $name:ident => $seg:expr;)*) => {$(
        #[$doc]
        pub fn $name() -> Result<PathBuf> { Ok(root()?.join($seg)) }
    )*};
}

sub! {
    /// Heruntergeladene Minecraft-Versionen: `versions/<id>/`
    versions => "versions";
    /// Gemeinsame Bibliotheken, nach Maven-Koordinaten abgelegt.
    libraries => "libraries";
    /// Spielressourcen: `assets/indexes/` und `assets/objects/`
    assets => "assets";
    /// Eine Ebene je Instanz: `instances/<uuid>/`
    instances => "instances";
    /// Geloeschte Instanzen, bis sie endgueltig entfernt werden.
    trash => "trash";
    /// Sicherungen: `snapshots/<instanz>/<id>.zip`
    snapshots => "snapshots";
    /// Entpackte native Bibliotheken je Version.
    natives => "natives";
    /// Selbst verwaltete Java-Installationen.
    jdks => "jdks";
    /// Protokolle.
    logs => "logs";
    /// Bedrock-Instanzen: je Instanz `instance.json` + eigene `data/`/`cache/`.
    bedrock_instances => "bedrock-instances";
}

/// Ordner einer einzelnen Bedrock-Instanz.
pub fn bedrock_instance_dir(id: &str) -> Result<PathBuf> {
    safe_join(&bedrock_instances()?, id)
}

pub fn config_file() -> Result<PathBuf> { Ok(root()?.join("config.json")) }
pub fn accounts_file() -> Result<PathBuf> { Ok(root()?.join("accounts.json")) }
pub fn servers_file() -> Result<PathBuf> { Ok(root()?.join("servers.json")) }

/// Ist `name` ein einzelner Name ohne Verzeichnisanteil?
///
/// Schuetzt vor Pfad-Traversal (`../../etc/passwd`) und vor absoluten
/// Pfaden (`/etc/passwd`), die `Path::join` sonst unveraendert durchliesse.
/// Wichtig ueberall dort, wo eine ID oder ein Dateiname aus der
/// Oberflaeche kommt und Teil eines Pfades wird -- Instanz- und
/// Versions-IDs sind intern zwar immer UUIDs bzw. Versionsnummern, aber
/// der Tauri-Befehl selbst kennt diese Herkunft nicht und nimmt jede
/// Zeichenkette entgegen.
pub fn is_bare_name(name: &str) -> bool {
    use std::path::Component;
    !name.is_empty()
        && matches!(
            Path::new(name).components().collect::<Vec<_>>().as_slice(),
            [Component::Normal(_)]
        )
}

/// Haengt `name` sicher an `base` an -- lehnt Pfad-Traversal und absolute
/// Pfade ab, statt sie stillschweigend durchzulassen.
pub fn safe_join(base: &Path, name: &str) -> Result<PathBuf> {
    if !is_bare_name(name) {
        return Err(Error::other(format!("Ungültiger Name: {name}")));
    }
    Ok(base.join(name))
}

pub fn instance_dir(id: &str) -> Result<PathBuf> { safe_join(&instances()?, id) }

/// Der Ordner, den Minecraft als Spielverzeichnis sieht.
///
/// **Das ist der Instanzordner selbst**, nicht ein Unterordner darin.
/// So legt es Version 0.1.3 an: `mods/`, `config/`, `saves/` und
/// `options.txt` liegen direkt neben der `instance.json`.
///
/// Ich hatte hier zunaechst `minecraft/` angenommen. Die Folge war, dass
/// der Launcher in einen leeren, selbst angelegten Ordner sah, null Mods
/// meldete und das Spiel ohne sie startete -- es sah aus, als waere eine
/// gewachsene Instanz zurueckgesetzt worden, obwohl nichts verloren war.
/// Deshalb steht es hier so ausdruecklich.
pub fn instance_game_dir(id: &str) -> Result<PathBuf> {
    instance_dir(id)
}

pub fn instance_config(id: &str) -> Result<PathBuf> {
    Ok(instance_dir(id)?.join("instance.json"))
}

pub fn version_dir(id: &str) -> Result<PathBuf> { safe_join(&versions()?, id) }

pub fn version_json(id: &str) -> Result<PathBuf> {
    Ok(version_dir(id)?.join(format!("{id}.json")))
}

pub fn version_jar(id: &str) -> Result<PathBuf> {
    Ok(version_dir(id)?.join(format!("{id}.jar")))
}

/// Legt das Verzeichnis samt Elternebenen an, falls es fehlt.
pub fn ensure_dir(p: &Path) -> Result<()> {
    if !p.exists() {
        std::fs::create_dir_all(p)?;
    }
    Ok(())
}

/// Legt das Elternverzeichnis einer Datei an.
pub fn ensure_parent(p: &Path) -> Result<()> {
    if let Some(parent) = p.parent() {
        ensure_dir(parent)?;
    }
    Ok(())
}

/// Wandelt Maven-Koordinaten in den Pfad unterhalb von `libraries/`.
///
/// `com.mojang:authlib:6.0.54` wird zu
/// `com/mojang/authlib/6.0.54/authlib-6.0.54.jar`.
/// Ein vierter Abschnitt ist der Klassifizierer:
/// `org.lwjgl:lwjgl:3.3.3:natives-linux` haengt `-natives-linux` an.
pub fn maven_path(coords: &str) -> Result<PathBuf> {
    let mut parts = coords.split(':');
    let group = parts.next().ok_or_else(|| Error::other("Leere Koordinaten"))?;
    let artifact = parts
        .next()
        .ok_or_else(|| Error::other(format!("Koordinaten ohne Artefakt: {coords}")))?;
    let version = parts
        .next()
        .ok_or_else(|| Error::other(format!("Koordinaten ohne Version: {coords}")))?;
    let classifier = parts.next();

    // Die Endung kann am letzten Abschnitt haengen: "…@zip"
    let (version, ext) = match version.split_once('@') {
        Some((v, e)) => (v, e),
        None => (version, "jar"),
    };

    let mut file = format!("{artifact}-{version}");
    if let Some(c) = classifier {
        let c = c.split('@').next().unwrap_or(c);
        file.push('-');
        file.push_str(c);
    }
    file.push('.');
    file.push_str(ext);

    let mut p = PathBuf::new();
    for seg in group.split('.') {
        p.push(seg);
    }
    p.push(artifact);
    p.push(version);
    p.push(file);
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::{is_bare_name, maven_path};

    #[test]
    fn normale_namen_sind_erlaubt() {
        assert!(is_bare_name("abc-123"));
        assert!(is_bare_name("1.21.4"));
        assert!(is_bare_name("fabric-loader-0.19.5-1.21.11"));
        assert!(is_bare_name("sodium-fabric-0.5.jar"));
    }

    #[test]
    fn pfad_traversal_wird_abgelehnt() {
        assert!(!is_bare_name("../etc/passwd"));
        assert!(!is_bare_name("../../etc/passwd"));
        assert!(!is_bare_name("a/../../b"));
        assert!(!is_bare_name(".."));
        assert!(!is_bare_name("."));
    }

    #[test]
    fn absolute_pfade_werden_abgelehnt() {
        // `Path::join` mit einem absoluten Pfad verwirft sonst die Basis
        // komplett -- genau das darf ein Dateiname/eine ID nie ausloesen.
        assert!(!is_bare_name("/etc/passwd"));
    }

    #[test]
    fn eingebettete_trenner_werden_abgelehnt() {
        assert!(!is_bare_name("a/b"));
        assert!(!is_bare_name(""));
    }

    #[test]
    fn maven_koordinaten() {
        assert_eq!(
            maven_path("com.mojang:authlib:6.0.54").unwrap(),
            std::path::Path::new("com/mojang/authlib/6.0.54/authlib-6.0.54.jar")
        );
        assert_eq!(
            maven_path("org.lwjgl:lwjgl:3.3.3:natives-linux").unwrap(),
            std::path::Path::new("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-linux.jar")
        );
        assert_eq!(
            maven_path("net.fabricmc:intermediary:1.21.4@zip").unwrap(),
            std::path::Path::new("net/fabricmc/intermediary/1.21.4/intermediary-1.21.4.zip")
        );
    }
}
