fn main() {
    match buildmc_launcher_lib::instance::list() {
        Ok(v) => {
            println!("Instanzen gelesen: {}", v.len());
            for i in &v {
                println!("  {:<18} {:<9} {:<8} last_played={:?}",
                    i.name, i.mc_version, i.loader,
                    if i.last_played.is_empty() { "—" } else { &i.last_played[..10] });
            }
        }
        Err(e) => println!("FEHLER: {e}"),
    }
}
