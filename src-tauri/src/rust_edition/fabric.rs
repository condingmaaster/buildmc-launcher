//! BHH: Fabric-Mods einer Instanz und ihre Kompatibilitaet.
//!
//! Die Pruefung ist mechanisch, nicht geschaetzt: aus dem Bytecode jeder Mod-Klasse
//! werden die benutzten Klassen gelesen (Konstantenpool) und mit den Fassaden
//! verglichen, die `bhh-api.jar` wirklich enthaelt. Was fehlt, steht in der Liste.
//! Mixins werden noch nicht unterstuetzt und als Grund genannt.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use super::{engine, Variant};
use crate::error::{Error, Result};

#[derive(Debug, Clone, Serialize)]
pub struct FabricMod {
    pub file: String,
    pub id: String,
    pub name: String,
    pub version: String,
    /// "bereit": alle benutzten Klassen vorhanden, keine Mixins, keine Client-Entrypoints.
    /// "unvollstaendig": laeuft vielleicht teilweise; Gruende in `reasons`.
    /// "kein_fabric_mod": keine `fabric.mod.json`.
    pub status: String,
    pub reasons: Vec<String>,
    /// Benutzte Minecraft-/Fabric-Klassen, die BHH noch nicht anbietet.
    pub missing_classes: Vec<String>,
    pub used_classes: usize,
}

/// Klassen, die `bhh-api.jar` anbietet (Fassaden + BHH selbst).
fn angebotene_klassen() -> Result<BTreeSet<String>> {
    let jar = engine::binary(Variant::Bhh, "bhh-api.jar")?;
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&jar)
        .map_err(|_| Error::other("bhh-api.jar fehlt -- BHH zuerst bauen"))?)
        .map_err(|e| Error::other(format!("bhh-api.jar: {e}")))?;
    let mut out = BTreeSet::new();
    for i in 0..zip.len() {
        if let Ok(f) = zip.by_index(i) {
            if let Some(n) = f.name().strip_suffix(".class") {
                // Innere Klassen zaehlen ueber ihre aeussere.
                out.insert(n.split('$').next().unwrap_or(n).to_string());
            }
        }
    }
    Ok(out)
}

/// Klassennamen aus dem Konstantenpool einer .class-Datei (CONSTANT_Class -> Utf8).
fn benutzte_klassen(class: &[u8]) -> Vec<String> {
    let rd = |i: usize| -> Option<u16> { class.get(i..i + 2).map(|b| u16::from_be_bytes([b[0], b[1]])) };
    if class.get(0..4) != Some(&[0xCA, 0xFE, 0xBA, 0xBE]) {
        return Vec::new();
    }
    let Some(anzahl) = rd(8) else { return Vec::new() };
    let mut utf8: Vec<Option<String>> = vec![None; anzahl as usize];
    let mut klassen_idx = Vec::new();
    let mut p = 10usize;
    let mut i = 1usize;
    while i < anzahl as usize {
        let Some(&tag) = class.get(p) else { break };
        p += 1;
        match tag {
            1 => {
                let Some(len) = rd(p) else { break };
                let s = class.get(p + 2..p + 2 + len as usize).map(|b| String::from_utf8_lossy(b).into_owned());
                utf8[i] = s;
                p += 2 + len as usize;
            }
            7 => { klassen_idx.push(rd(p).unwrap_or(0) as usize); p += 2; }
            8 | 16 | 19 | 20 => p += 2,
            15 => p += 3,
            3 | 4 | 9 | 10 | 11 | 12 | 17 | 18 => p += 4,
            5 | 6 => { p += 8; i += 1; } // long/double belegen zwei Eintraege
            _ => break,
        }
        i += 1;
    }
    klassen_idx.into_iter().filter_map(|k| utf8.get(k).cloned().flatten())
        .map(|n| n.trim_start_matches('[').trim_start_matches('L').trim_end_matches(';').to_string())
        .filter(|n| n.starts_with("net/minecraft/") || n.starts_with("com/mojang/") || n.starts_with("net/fabricmc/"))
        .map(|n| n.split('$').next().unwrap_or(&n).to_string())
        .collect()
}

/// Fabric-Module, die BHH ueberhaupt bedient (und damit als Abhaengigkeit erfuellt).
const ERFUELLT: &[&str] = &["fabricloader", "java", "minecraft", "fabric-api-base", "fabric-lifecycle-events-v1"];

fn pruefen(jar: &Path, angeboten: &BTreeSet<String>) -> FabricMod {
    let file = jar.file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
    match std::fs::read(jar) {
        Ok(bytes) => pruefen_bytes(&file, bytes, angeboten, 0),
        Err(_) => leer(&file, "kein_fabric_mod", "Datei nicht lesbar".into()),
    }
}

fn leer(file: &str, status: &str, grund: String) -> FabricMod {
    FabricMod {
        file: file.into(), id: String::new(), name: file.into(), version: String::new(),
        status: status.into(), reasons: vec![grund], missing_classes: Vec::new(), used_classes: 0,
    }
}

/// Prueft eine Mod-Jar aus dem Speicher -- auch eingebettete Jars (Jar-in-Jar, `"jars"` in
/// `fabric.mod.json`), wie sie Fabric API und viele Bibliotheken als Buendel mitbringen.
fn pruefen_bytes(file: &str, bytes: Vec<u8>, angeboten: &BTreeSet<String>, tiefe: u32) -> FabricMod {
    let Ok(mut zip) = zip::ZipArchive::new(std::io::Cursor::new(bytes)) else { return leer(file, "kein_fabric_mod", "keine Jar-Datei".into()) };
    let mut text = String::new();
    let json: serde_json::Value = match zip.by_name("fabric.mod.json") {
        Ok(mut e) => { let _ = e.read_to_string(&mut text); serde_json::from_str(&text).unwrap_or_default() }
        Err(_) => return leer(file, "kein_fabric_mod", "keine fabric.mod.json -- kein Fabric-Mod".into()),
    };

    let mut gruende = Vec::new();
    let mixins = json["mixins"].as_array().map(|a| a.len()).unwrap_or(0);
    if mixins > 0 {
        gruende.push(format!("benutzt {mixins} Mixin-Konfiguration(en) -- Mixins unterstützt BHH noch nicht"));
    }
    if json["entrypoints"]["client"].as_array().is_some_and(|a| !a.is_empty()) {
        gruende.push("hat Client-Entrypoints -- die Client-Seite (Bildschirme, HUD) hat BHH noch nicht".into());
    }
    if let Some(dep) = json["depends"].as_object() {
        let offen: Vec<&String> = dep.keys().filter(|k| !ERFUELLT.contains(&k.as_str()) && *k != "fabric" && *k != "fabric-api").collect();
        if !offen.is_empty() {
            gruende.push(format!("braucht noch nicht bediente Module: {}", offen.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")));
        }
    }

    let mut benutzt = BTreeSet::new();
    let mut eingebettet = Vec::new();
    let verschachtelt: BTreeSet<String> = json["jars"].as_array().map(|a| a.iter()
        .filter_map(|j| j["file"].as_str().map(str::to_string)).collect()).unwrap_or_default();
    for i in 0..zip.len() {
        let Ok(mut e) = zip.by_index(i) else { continue };
        let name = e.name().to_string();
        let mut buf = Vec::new();
        if name.ends_with(".class") {
            if e.read_to_end(&mut buf).is_ok() {
                benutzt.extend(benutzte_klassen(&buf));
            }
        } else if tiefe < 3 && verschachtelt.contains(&name) && e.read_to_end(&mut buf).is_ok() {
            eingebettet.push(pruefen_bytes(&name, buf, angeboten, tiefe + 1));
        }
    }
    let mut fehlend: BTreeSet<String> = benutzt.iter().filter(|k| !angeboten.contains(*k)).cloned().collect();
    let mut gesamt_benutzt = benutzt.len();
    if !eingebettet.is_empty() {
        let offen: Vec<&FabricMod> = eingebettet.iter().filter(|m| m.status != "bereit").collect();
        for m in &eingebettet {
            fehlend.extend(m.missing_classes.iter().cloned());
            gesamt_benutzt += m.used_classes;
        }
        if !offen.is_empty() {
            gruende.push(format!("{} von {} eingebetteten Modulen noch nicht bereit (z. B. {})", offen.len(), eingebettet.len(),
                offen.iter().take(3).map(|m| if m.id.is_empty() { m.file.as_str() } else { m.id.as_str() }).collect::<Vec<_>>().join(", ")));
        }
    }
    if !fehlend.is_empty() {
        gruende.push(format!("{} benutzte Minecraft-/Fabric-Klasse(n) hat BHH noch nicht", fehlend.len()));
    }

    FabricMod {
        file: file.rsplit('/').next().unwrap_or(file).into(),
        id: json["id"].as_str().unwrap_or("").into(),
        name: json["name"].as_str().or(json["id"].as_str()).unwrap_or("").into(),
        version: json["version"].as_str().unwrap_or("").into(),
        status: if gruende.is_empty() { "bereit" } else { "unvollstaendig" }.into(),
        reasons: gruende,
        missing_classes: fehlend.into_iter().collect(),
        used_classes: gesamt_benutzt,
    }
}

/// Alle Mods einer BHH-Instanz mit Pruefergebnis.
pub fn list(instance_id: &str) -> Result<Vec<FabricMod>> {
    let i = super::load(Variant::Bhh, instance_id)?;
    let dir = i.fabric_mods_dir()?;
    crate::paths::ensure_dir(&dir)?;
    let angeboten = angebotene_klassen().unwrap_or_default();
    let mut jars: Vec<_> = std::fs::read_dir(&dir)?.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jar")).collect();
    jars.sort();
    Ok(jars.iter().map(|j| pruefen(j, &angeboten)).collect())
}

/// Alle Jars eines Ordners pruefen (Kompatibilitaetsmatrix, auch ohne Instanz).
pub fn matrix(dir: &Path) -> Result<Vec<FabricMod>> {
    let angeboten = angebotene_klassen()?;
    let mut jars: Vec<_> = std::fs::read_dir(dir)?.filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|x| x == "jar")).collect();
    jars.sort();
    Ok(jars.iter().map(|j| pruefen(j, &angeboten)).collect())
}

/// Eine Mod-Jar in die Instanz kopieren.
pub fn add(instance_id: &str, source: &Path) -> Result<FabricMod> {
    let i = super::load(Variant::Bhh, instance_id)?;
    let dir = i.fabric_mods_dir()?;
    crate::paths::ensure_dir(&dir)?;
    let name = source.file_name().ok_or_else(|| Error::other("kein Dateiname"))?;
    if source.extension().is_none_or(|x| x != "jar") {
        return Err(Error::other("Nur .jar-Dateien"));
    }
    let ziel = dir.join(name);
    std::fs::copy(source, &ziel)?;
    Ok(pruefen(&ziel, &angebotene_klassen().unwrap_or_default()))
}

pub fn remove(instance_id: &str, file: &str) -> Result<()> {
    if !crate::paths::is_bare_name(file) {
        return Err(Error::other("ungültiger Dateiname"));
    }
    let i = super::load(Variant::Bhh, instance_id)?;
    std::fs::remove_file(i.fabric_mods_dir()?.join(file))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Gegen die echten Jars aus `bhh/java/build.sh` (wird uebersprungen, wenn nicht gebaut).
    #[test]
    fn prueft_echte_bhh_jars() {
        let build = Path::new(env!("CARGO_MANIFEST_DIR")).join("../bhh/java/build");
        let Ok(api) = std::fs::File::open(build.join("bhh-api.jar")) else { return };
        let mut zip = zip::ZipArchive::new(api).unwrap();
        let angeboten: BTreeSet<String> = (0..zip.len()).filter_map(|i| zip.by_index(i).ok()
            .and_then(|f| f.name().strip_suffix(".class").map(|n| n.split('$').next().unwrap().to_string()))).collect();
        assert!(angeboten.contains("net/minecraft/server/MinecraftServer"));

        let tick = pruefen(&build.join("bhh-tickmod.jar"), &angeboten);
        assert_eq!(tick.id, "bhh_tickgreeter");
        assert_eq!(tick.status, "bereit", "{:?}", tick.reasons);
        assert!(tick.used_classes >= 4, "erkannte Klassen: {}", tick.used_classes);

        // Ohne die Server-Fassaden muss genau das als fehlend erscheinen.
        let ohne: BTreeSet<String> = angeboten.iter().filter(|k| !k.starts_with("net/minecraft/")).cloned().collect();
        let t2 = pruefen(&build.join("bhh-tickmod.jar"), &ohne);
        assert_eq!(t2.status, "unvollstaendig");
        assert!(t2.missing_classes.contains(&"net/minecraft/server/MinecraftServer".to_string()), "{:?}", t2.missing_classes);
    }
}
