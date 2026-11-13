//! Manueller Netzwerktest: XDG_DATA_HOME auf ein leeres Testverzeichnis setzen.
use buildmc_launcher_lib::{download, instance, modrinth, packs, paths};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let test_root = std::env::var_os("BUILDMC_SHADER_TEST_ROOT")
        .map(std::path::PathBuf::from).expect("BUILDMC_SHADER_TEST_ROOT fehlt");
    assert!(paths::root()?.starts_with(&test_root), "Nur in isoliertem Testverzeichnis ausführen");
    let instance = instance::create("Shader-Netzwerktest", "1.21.1", "buildmc", false)?;
    let results = modrinth::search("Complementary", Some("1.21.1"), Some("buildmc"), "shader", 0).await?;
    assert!(!results.results.is_empty());
    assert!(results.results.iter().all(|p| p.project_type == "shader"));
    let project = &results.results[0];
    let versions = modrinth::versions(&project.project_id, Some("1.21.1"), None).await?;
    let selected = versions.iter().find(|v| v.release_type == "release").or(versions.first()).expect("Shaderfassung fehlt");
    let filename = packs::install_shader(&instance.id, &selected.version_id, std::sync::Arc::new(download::Silent)).await?;
    let file = instance.game_dir()?.join("shaderpacks").join(&filename);
    assert!(file.is_file());
    assert!(!instance.game_dir()?.join("mods").join(&filename).exists());
    let mut zip = zip::ZipArchive::new(std::fs::File::open(file)?)?;
    assert!((0..zip.len()).any(|i| zip.by_index(i).unwrap().name().starts_with("shaders/")));
    assert!(instance::load(&instance.id)?.shader_packs.contains(&filename));
    println!("PASS: {} als {} in shaderpacks, gültiges Shader-ZIP und Instanzzuordnung geprüft", project.title, filename);
    Ok(())
}
