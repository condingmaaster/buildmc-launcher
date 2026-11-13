//! Ping gegen echte Server.
use buildmc_launcher_lib as lib;

fn main() {
    for adresse in ["play.buildmc.io", "mc.hypixel.net", "localhost:25565", "127.0.0.1:1"] {
        let s = lib::server::ping(adresse);
        if s.online {
            println!("{adresse:<22} ONLINE  {}/{} Spieler  {} ms  [{}]",
                s.players_online, s.players_max, s.latency_ms, s.version);
            if !s.motd.is_empty() {
                println!("{:<22} MOTD: {}", "", s.motd.chars().take(60).collect::<String>());
            }
            println!("{:<22} Symbol: {}", "", if s.favicon.is_some() { "ja" } else { "nein" });
        } else {
            println!("{adresse:<22} offline / nicht erreichbar");
        }
    }
}
