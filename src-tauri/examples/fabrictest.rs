//! Fabric installieren, eine Mod dazulegen, starten.

use std::sync::Arc;
use buildmc_launcher_lib as lib;
use lib::download::{Progress, Sink};
use lib::launch::process::Events;
use lib::loader::LoaderKind;

struct P;
impl Sink for P {
    fn emit(&self, p: &Progress) {
        if p.done { println!("   [{}] fertig ({} Dateien)", p.label, p.total); }
    }
}
struct L;
impl Events for L {
    fn log_line(&self, _: &str, line: &str) { println!("  | {line}"); }
    fn stopped(&self, id: &str, code: i32, s: u64) {
        println!("\n>>> beendet: Code {code}, {s}s");
        if code != 0 {
            if let Ok(Some(d)) = lib::launch::analyze_instance(id) {
                println!(">>> Diagnose: {}", d.cause);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mc = "1.21.4";
    let name = "Fabrictest";

    let mut instance = match lib::instance::list()?.into_iter().find(|i| i.name == name) {
        Some(i) => { println!("Vorhandene Instanz: {}", i.id); i }
        None => { let i = lib::instance::create(name, mc, "fabric", false)?;
                  println!("Instanz angelegt: {}", i.id); i }
    };

    println!("\n1. Fabric-Fassungen");
    let list = lib::loader::versions(LoaderKind::Fabric, mc).await?;
    println!("   {} verfügbar, stabil oben:", list.len());
    for v in list.iter().take(3) {
        println!("     {:<10} stabil={} installiert={}", v.version, v.stable, v.installed);
    }

    let empfohlen = lib::loader::recommended(LoaderKind::Fabric, mc).await?;
    println!("   empfohlen: {empfohlen}");

    println!("\n2. Installieren");
    let id = lib::loader::install(LoaderKind::Fabric, mc, &empfohlen).await?;
    println!("   Profil abgelegt als: {id}");

    instance.fabric_version = Some(empfohlen.clone());
    lib::instance::save(&instance)?;
    println!("   Instanz startet nun: {}", instance.launch_version());

    println!("\n3. Vererbung auflösen");
    let info = lib::manifest::resolve(&instance.launch_version()).await?;
    println!("   Hauptklasse: {}", info.main_class);
    println!("   Bibliotheken nach Vererbung: {}", info.libraries.len());
    println!("   Asset-Index: {} | Java: {}", info.asset_index_id(), info.required_java());

    println!("\n4. Sodium dazu");
    let versions = lib::modrinth::versions("sodium", Some(mc), Some("fabric")).await?;
    if let Some(v) = versions.iter().find(|v| v.release_type == "release") {
        lib::mods::install(&instance.id, &v.version_id, Some(&v.provider), Arc::new(P)).await?;
        println!("   {} installiert", v.version_number);
    }

    println!("\n5. Starten");
    match lib::launch::start(&instance.id, lib::launch::JoinTarget::None,
                             Arc::new(P), Arc::new(L)).await {
        Ok(r) => {
            println!(">>> LÄUFT: PID {}\n", r.pid);
            tokio::time::sleep(std::time::Duration::from_secs(75)).await;
            println!("\n>>> Beobachtung endet.");
        }
        Err(e) => println!(">>> FEHLGESCHLAGEN: {e}"),
    }
    Ok(())
}
