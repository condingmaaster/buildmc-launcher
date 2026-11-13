//! Start ohne Oberflaeche (Fehlersuche): cargo run --example rust_launch -- [horizon|bhh] [sekunden]
use std::sync::Arc;
use buildmc_launcher_lib::rust_edition::{self as re, launch};

struct Ev;
impl launch::RustEvents for Ev {
    fn log_line(&self, _: &str, line: &str) { eprintln!("  | {line}"); }
    fn stopped(&self, _: &str, code: i32, secs: u64, crash: Option<String>) {
        eprintln!("BEENDET code={code} nach {secs}s absturzbericht={crash:?}");
    }
}

#[tokio::main]
async fn main() {
    let v = re::Variant::parse(std::env::args().nth(1).as_deref());
    let sek: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    let name = std::env::args().nth(3).unwrap_or_else(|| "Starttest".into());
    let inst = match re::list(v).unwrap().into_iter().find(|i| i.name == name) {
        Some(i) => i,
        None => re::create(v, "Starttest", "").unwrap(),
    };
    match launch::start(v, &inst.id, Arc::new(Ev)).await {
        Ok(r) => eprintln!("GESTARTET pid {}", r.pid),
        Err(e) => { eprintln!("START FEHLGESCHLAGEN: {e}"); return; }
    }
    tokio::time::sleep(std::time::Duration::from_secs(sek)).await;
    eprintln!("läuft noch: {}", launch::is_running(&inst.id));
    let _ = launch::stop(&inst.id);
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
}
