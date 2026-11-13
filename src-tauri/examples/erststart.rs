//! Erster echter Startversuch -- die gesamte Kette ohne Oberflaeche.
//!
//! Legt eine eigene Testinstanz an und laesst bestehende unberuehrt.

use std::sync::Arc;

use buildmc_launcher_lib as lib;
use lib::download::{Progress, Sink};
use lib::launch::process::Events;

struct Printer;

impl Sink for Printer {
    fn emit(&self, p: &Progress) {
        if p.done {
            println!("  [{}] fertig ({} Dateien)", p.label, p.total);
        } else if p.total > 0 && p.current % 200 == 0 {
            println!(
                "  [{}] {}/{}  {:.1} MB/s",
                p.label, p.current, p.total,
                p.bytes_per_second as f64 / 1_048_576.0
            );
        }
    }
}

struct Logger;

impl Events for Logger {
    fn log_line(&self, _id: &str, line: &str) {
        println!("  | {line}");
    }
    fn stopped(&self, id: &str, code: i32, played: u64) {
        println!("\n>>> beendet: {id}, Code {code}, {played}s gelaufen");
        if code != 0 {
            if let Ok(Some(d)) = lib::launch::analyze_instance(id) {
                println!(">>> Diagnose: {}", d.cause);
                println!("    {}", d.explanation.lines().next().unwrap_or(""));
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let version = std::env::args().nth(1).unwrap_or_else(|| "1.21.4".into());
    println!("=== Startversuch mit Minecraft {version} ===\n");

    // 1. Konten
    let accounts = lib::store::load_accounts();
    println!("Konten gelesen: {}", accounts.len());
    for a in &accounts {
        println!(
            "  {} ({}), Token gültig: {}",
            a.username, a.account_type, a.token_valid()
        );
    }
    if accounts.is_empty() {
        println!("FEHLER: keine Konten lesbar -- das war der Fehler mit expires_at");
        return Ok(());
    }

    // 2. Testinstanz -- bestehende bleiben unberuehrt
    let name = format!("Starttest {version}");
    let existing = lib::instance::list()?.into_iter().find(|i| i.name == name);
    let instance = match existing {
        Some(i) => {
            println!("\nVorhandene Testinstanz: {}", i.id);
            i
        }
        None => {
            let i = lib::instance::create(&name, &version, "vanilla", false)?;
            println!("\nTestinstanz angelegt: {}", i.id);
            i
        }
    };

    // 3. Versions-JSON und Java
    let info = lib::manifest::resolve(&instance.launch_version()).await?;
    println!("  Hauptklasse: {}", info.main_class);
    println!("  Asset-Index: {}", info.asset_index_id());
    println!("  Java nötig:  {}", info.required_java());

    let config = lib::store::load_config();
    match lib::launch::resolve_java(&instance, &config, &info) {
        Ok(j) => println!("  Java gewählt: {} ({})", j.version, j.path),
        Err(e) => {
            println!("  FEHLER bei Java: {e}");
            return Ok(());
        }
    }

    // 4. Starten
    println!("\n--- Dateien prüfen und starten ---");
    match lib::launch::start(
        &instance.id,
        lib::launch::JoinTarget::None,
        Arc::new(Printer),
        Arc::new(Logger),
    )
    .await
    {
        Ok(r) => {
            println!("\n>>> LÄUFT: PID {}, seit {}", r.pid, r.started_at);
            println!(">>> Protokoll folgt. Abbruch mit Strg+C.\n");
            // Laufen lassen, damit das Spielfenster erscheint.
            tokio::time::sleep(std::time::Duration::from_secs(150)).await;
            println!("\n>>> Beobachtung endet, Spiel läuft weiter.");
        }
        Err(e) => println!("\n>>> START FEHLGESCHLAGEN: {e}"),
    }

    Ok(())
}
