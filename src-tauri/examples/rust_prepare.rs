//! Vorbereiten ohne Oberflaeche (Fehlersuche): cargo run --example rust_prepare -- [horizon|bhh]
use std::sync::Arc;

struct Ausgabe;
impl buildmc_launcher_lib::download::Sink for Ausgabe {
    fn emit(&self, p: &buildmc_launcher_lib::download::Progress) {
        eprintln!("{:?} [{}] {} {}/{} {} {}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() % 10000,
            p.id, p.label, p.current, p.total, p.detail, p.error.clone().unwrap_or_default());
    }
}

#[tokio::main]
async fn main() {
    let v = buildmc_launcher_lib::rust_edition::Variant::parse(std::env::args().nth(1).as_deref());
    let t = std::time::Instant::now();
    let r = buildmc_launcher_lib::rust_edition::prepare::prepare(v, Arc::new(Ausgabe)).await;
    eprintln!("Ergebnis nach {:?}: {:?}", t.elapsed(), r.map(|_| "ok"));
}
