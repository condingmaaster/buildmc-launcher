//! Prueft die Modverwaltung gegen die echte Modrinth-API.

use std::sync::Arc;
use buildmc_launcher_lib as lib;
use lib::download::{Progress, Sink};

struct Quiet;
impl Sink for Quiet {
    fn emit(&self, p: &Progress) {
        if p.done { println!("      geladen ({} Dateien)", p.total); }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mc = "1.21.4";
    let loader = "fabric";

    println!("1. Suche");
    let page = lib::modrinth::search("sodium", Some(mc), Some(loader), "mod", 0).await?;
    println!("   {} Treffer gesamt", page.total);
    for r in page.results.iter().take(3) {
        println!("   - {:<24} {:>12} Downloads  Symbol: {}",
            r.title, r.downloads, if r.icon_url.is_some() { "ja" } else { "nein" });
        println!("     Loader: {:?}  Kategorien: {:?}", r.loaders, r.categories);
    }

    let first = page.results.first().ok_or("keine Treffer")?;

    println!("\n2. Fassungen von {}", first.title);
    let versions = lib::modrinth::versions(&first.project_id, Some(mc), Some(loader)).await?;
    println!("   {} passende Fassungen", versions.len());
    let newest = versions.iter().find(|v| v.release_type == "release")
        .or_else(|| versions.first()).ok_or("keine Fassung")?;
    println!("   neueste: {} ({}), {} Bytes, {} Abhängigkeiten",
        newest.version_number, newest.release_type, newest.size_bytes,
        newest.dependencies.len());

    // Instanz aus dem Startversuch weiterverwenden
    let instance = lib::instance::list()?.into_iter()
        .find(|i| i.name.starts_with("Starttest"))
        .ok_or("Testinstanz fehlt")?;
    println!("\n3. Plan für Instanz „{}“", instance.name);

    let plan = lib::mods::plan::resolve(
        &instance.id, &newest.version_id, &instance.mc_version, &instance.loader).await?;
    println!("   zu installieren: {}", plan.install.len());
    for v in &plan.install { println!("     + {} {}", v.name, v.version_number); }
    println!("   bereits da: {:?}", plan.already_present);
    println!("   Konflikte: {}", plan.conflicts.len());
    for c in &plan.conflicts { println!("     ! [{}] {}: {}", c.kind, c.subject, c.detail); }
    println!("   Gesamtgröße: {} KB", plan.total_bytes / 1024);

    println!("\n4. Installation");
    for v in &plan.install {
        println!("   {} …", v.name);
        lib::mods::install(&instance.id, &v.version_id, Some(&v.provider), Arc::new(Quiet)).await?;
    }

    println!("\n5. Liste der Instanz");
    let mut mods = lib::mods::list(&instance.id)?;
    lib::mods::enrich(&instance.id, &mut mods).await?;
    for m in &mods {
        println!("   {:<22} {:<14} an={} Symbol={} Projekt={:?}",
            m.name, m.version, m.enabled,
            if m.icon_url.is_some() { "ja " } else { "nein" },
            m.project_id.as_deref().unwrap_or("-"));
    }

    println!("\n6. Umschalten");
    if let Some(m) = mods.first() {
        let neu = lib::mods::toggle(&instance.id, &m.filename)?;
        println!("   {} -> {}", m.filename, neu);
        let zurueck = lib::mods::toggle(&instance.id, &neu)?;
        println!("   {} -> {}", neu, zurueck);
    }

    println!("\n7. Update-Prüfung");
    let checked = lib::mods::check_updates(&instance.id, mc, loader).await?;
    let offen: Vec<_> = checked.iter().filter(|m| m.update_available.is_some()).collect();
    println!("   {} von {} Mods haben Updates", offen.len(), checked.len());

    Ok(())
}
