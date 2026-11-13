//! Kompatibilitaetsmatrix eines Mod-Ordners gegen die gebaute bhh-api.jar.
//! cargo run --example bhh_matrix -- <ordner>
fn main() {
    let dir = std::env::args().nth(1).expect("Ordner");
    let mods = buildmc_launcher_lib::rust_edition::fabric::matrix(std::path::Path::new(&dir)).expect("Matrix");
    let mut alle: std::collections::BTreeMap<String, usize> = Default::default();
    for m in &mods {
        println!("{:<16} {:<38} {:>5} Klassen, {:>5} fehlen | {}", m.status, format!("{} {}", m.id, m.version), m.used_classes, m.missing_classes.len(), m.reasons.join(" / "));
        for c in &m.missing_classes {
            *alle.entry(c.clone()).or_default() += 1;
        }
    }
    let mut top: Vec<_> = alle.into_iter().collect();
    top.sort_by(|a, b| b.1.cmp(&a.1));
    println!("\nAm häufigsten fehlende Klassen ({} verschiedene):", top.len());
    for (c, n) in top.iter().take(25) {
        println!("  {n:>3} Mods  {}", c.replace('/', "."));
    }
}
