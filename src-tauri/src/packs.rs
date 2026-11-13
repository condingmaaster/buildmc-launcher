//! Shaderpakete: ZIP unverändert in den Shaderordner der Zielinstanz.
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::download::{self, Item, Sink};
use crate::error::{Error, Result};
use crate::modrinth::{self, ModVersion};

fn shader_target(game_dir: &Path, version: &ModVersion, game_version: &str) -> Result<PathBuf> {
    if !version.game_versions.iter().any(|v| v == game_version) {
        return Err(Error::other(format!("Kein Shaderpaket für Minecraft {game_version}")));
    }
    if !version.filename.to_ascii_lowercase().ends_with(".zip")
        || !version.loaders.iter().any(|l| ["iris", "optifine"].contains(&l.as_str()))
    {
        return Err(Error::other("Diese Datei ist kein Iris-/OptiFine-Shaderpaket"));
    }
    crate::paths::safe_join(&game_dir.join("shaderpacks"), &version.filename)
}

pub async fn install_shader(instance_id: &str, version_id: &str, sink: Arc<dyn Sink>) -> Result<String> {
    let instance = crate::instance::load(instance_id)?;
    let version = modrinth::version(version_id).await?;
    let target = shader_target(&instance.game_dir()?, &version, &instance.mc_version)?;
    download::fetch_all(
        vec![Item {
            url: version.url.clone(), target, sha1: version.sha1.clone(), size: version.size_bytes,
        }],
        1, true, "Shaderpaket wird geladen", "shader", sink,
    ).await?;

    // Frisch laden: Einstellungen können sich während des Downloads ändern.
    let mut instance = crate::instance::load(instance_id)?;
    if !instance.shader_packs.contains(&version.filename) {
        instance.shader_packs.push(version.filename.clone());
        crate::instance::save(&instance)?;
    }
    Ok(version.filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shader() -> ModVersion {
        serde_json::from_value(serde_json::json!({
            "version_id":"test", "project_id":"shader", "provider":"modrinth",
            "name":"Shader", "version_number":"1", "changelog":null,
            "release_type":"release", "downloads":0, "published_at":"",
            "game_versions":["1.21.1"], "loaders":["iris", "optifine"],
            "filename":"Shader.zip", "url":"", "size_bytes":10,
            "sha1":null, "dependencies":[]
        })).unwrap()
    }

    #[test]
    fn shader_liegt_unveraendert_im_shaderordner_der_zielinstanz() {
        let base = std::env::temp_dir().join("buildmc-shader-test-instance");
        let target = shader_target(&base, &shader(), "1.21.1").unwrap();
        assert_eq!(target, base.join("shaderpacks").join("Shader.zip"));
        assert!(!target.starts_with(base.join("mods")));
    }

    #[test]
    fn falsche_spielversion_wird_abgewiesen() {
        assert!(shader_target(Path::new("instance"), &shader(), "1.20.1").is_err());
    }

    #[test]
    fn mod_oder_resourcepack_ist_kein_shaderpaket() {
        let mut version = shader();
        version.filename = "iris.jar".into();
        version.loaders = vec!["fabric".into()];
        assert!(shader_target(Path::new("instance"), &version, "1.21.1").is_err());
        version.filename = "resources.zip".into();
        version.loaders = vec!["minecraft".into()];
        assert!(shader_target(Path::new("instance"), &version, "1.21.1").is_err());
    }

    #[test]
    fn dateiname_darf_shaderordner_nicht_verlassen() {
        let mut version = shader();
        version.filename = "../mods/Shader.zip".into();
        assert!(shader_target(Path::new("instance"), &version, "1.21.1").is_err());
    }
}
