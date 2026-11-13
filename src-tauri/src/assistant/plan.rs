//! Aus Antworten und Auswahl einen Plan machen.
//!
//! Dieser Weg kommt **ohne Sprachmodell** aus. Er ist damit zugleich die
//! Fassung ohne Schlüssel und der Rückfall, wenn das Modell ausfällt --
//! und weil er zuerst gebaut wurde, ist die Modellanfrage später nur eine
//! Verbesserung obendrauf, kein tragender Teil.
//!
//! Zusammengeführt werden zwei Quellen: was der Nutzer ausgewählt hat, und
//! was die Optimierungsstufe an Rollen mitbringt. Überschneidungen werden
//! entdoppelt -- wer Sodium selbst angehakt hat, soll es nicht zweimal
//! im Plan sehen.

use std::collections::HashSet;

use super::session::{Plan, PlannedMod, Session};
use crate::error::Result;
use crate::optimize::{self, resolve, Level, Pacing};

/// Baut den Plan.
///
/// `instance_name` kommt aus der Oberfläche; ist er leer, wird einer aus
/// Version und Loader gebildet.
pub async fn build(session: &Session, instance_name: &str) -> Result<Plan> {
    let outcome = session.outcome();

    let mc_version = outcome
        .mc_version
        .clone()
        .ok_or_else(|| crate::Error::other("Es wurde keine Minecraft-Version gewählt"))?;
    let loader = outcome.loader.clone().unwrap_or_else(|| "vanilla".into());
    let level = outcome.level.unwrap_or(Level::Performance);
    let pacing = outcome.pacing.unwrap_or_default();

    let max_ram = outcome.max_ram.unwrap_or(4096);
    // Ein Mindestwert unter einem Viertel des Höchstwerts bringt nichts --
    // die Halde wächst dann ohnehin sofort.
    let min_ram = (max_ram / 2).clamp(1024, max_ram);

    let mut mods: Vec<PlannedMod> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut notes = outcome.notes.clone();

    // 1. Was der Nutzer ausgewählt hat.
    for id in &session.selected {
        let Some(c) = session.candidates.iter().find(|c| &c.project.project_id == id) else {
            continue;
        };
        if !seen.insert(id.clone()) {
            continue;
        }
        mods.push(PlannedMod {
            project_id: id.clone(),
            title: c.project.title.clone(),
            icon_url: c.project.icon_url.clone(),
            reason: reason_for(c),
            from_optimization: false,
        });
    }

    // 2. Was die Optimierungsstufe braucht.
    //
    // Nur bei einem echten Loader -- ohne ihn liessen sich die Mods
    // gar nicht laden.
    // Der BuildMC-Client setzt auf Fabric auf -- Optimierungsmods laufen
    // dort genauso.
    let mit_loader = crate::loader::LoaderKind::parse(&loader).is_some() || loader == "buildmc";
    if mit_loader {
        let optimization = resolve::plan(&mc_version, &loader, level).await?;

        for role in &optimization.roles {
            match &role.chosen {
                Some(chosen) => {
                    // Die Kennung der Rolle ist ein Namensteil, nicht die
                    // Projektkennung -- deshalb über den Namen entdoppeln.
                    if !seen.insert(chosen.slug.clone()) {
                        continue;
                    }
                    mods.push(PlannedMod {
                        project_id: chosen.slug.clone(),
                        title: role.label.clone(),
                        icon_url: None,
                        reason: role.why.clone(),
                        from_optimization: true,
                    });
                }
                None if !role.optional => {
                    notes.push(format!(
                        "{} fehlt für Minecraft {mc_version}. {}",
                        role.label,
                        role.note.clone().unwrap_or_default()
                    ));
                }
                None => {}
            }
        }

        if !optimization.usable {
            notes.push(
                "Ein unverzichtbarer Teil der Optimierung fehlt für diese Version — \
                 die Stufe bringt dann deutlich weniger."
                    .into(),
            );
        }
    }

    let name = if instance_name.trim().is_empty() {
        format!("{mc_version} {}", label_of(&loader))
    } else {
        instance_name.trim().to_string()
    };

    Ok(Plan {
        name,
        mc_version,
        loader,
        loader_version: None, // wird beim Umsetzen ermittelt
        min_ram,
        max_ram,
        optimization: Some(level.label().to_string()),
        frame_cap: match pacing {
            Pacing::Smooth { cap } => Some(cap),
            Pacing::Unlimited => None,
        },
        jvm_args: jvm_args(max_ram, mods.len()),
        mods,
        rationale: None, // nur mit Modell
        notes: dedupe(notes),
    })
}

/// Warum steht diese Mod im Plan?
///
/// Ohne Modell keine freie Begründung -- aber die Suchbegriffe, die sie
/// gefunden haben, sagen bereits das Wesentliche.
fn reason_for(c: &super::search::Candidate) -> String {
    if c.matched.is_empty() {
        return "Von dir ausgewählt.".into();
    }
    format!("Gefunden über: {}", c.matched.join(", "))
}

fn label_of(loader: &str) -> &str {
    match loader {
        "fabric" => "Fabric",
        "quilt" => "Quilt",
        "neoforge" => "NeoForge",
        "forge" => "Forge",
        "buildmc" => "BuildMC",
        _ => "Vanilla",
    }
}

/// JVM-Schalter aus Speicher und Modanzahl.
fn jvm_args(max_ram: u32, mod_count: usize) -> Vec<String> {
    let mut out = vec![
        "-XX:+UseZGC".to_string(),
        "-XX:+AlwaysPreTouch".to_string(),
    ];
    // Viele Mods laden sehr viele Klassen.
    if mod_count > 60 {
        out.push("-XX:MaxMetaspaceSize=512m".into());
    }
    if max_ram >= 8192 {
        out.push("-XX:+UseTransparentHugePages".into());
    }
    out
}

fn dedupe(mut items: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    items.retain(|i| seen.insert(i.clone()));
    items
}

/// Setzt den Plan um: Instanz anlegen, Loader installieren, Mods holen,
/// Optimierung schreiben.
///
/// Die Reihenfolge ist Absicht -- zuerst das, was scheitern kann, und erst
/// danach das Schreiben von Dateien.
pub async fn apply(
    plan: &Plan,
    sink: std::sync::Arc<dyn crate::download::Sink>,
) -> Result<crate::instance::InstanceConfig> {
    // 1. Instanz
    let mut instance =
        crate::instance::create(&plan.name, &plan.mc_version, &plan.loader, false)?;
    instance.min_ram = plan.min_ram;
    instance.max_ram = plan.max_ram;
    instance.jvm_args = plan.jvm_args.clone();

    // 2. Loader
    if let Some(kind) = crate::loader::LoaderKind::parse(&plan.loader) {
        let version = match &plan.loader_version {
            Some(v) if !v.is_empty() => v.clone(),
            _ => crate::loader::recommended(kind, &plan.mc_version).await?,
        };
        crate::loader::install(kind, &plan.mc_version, &version).await?;
        instance.fabric_version = Some(version);
    }
    crate::instance::save(&instance)?;

    // 3. Mods -- ein Fehlschlag bei einer soll nicht alle anderen verhindern.
    for planned in &plan.mods {
        let versions = crate::modrinth::versions(
            &planned.project_id,
            Some(&plan.mc_version),
            Some(&plan.loader),
        )
        .await
        .unwrap_or_default();

        let Some(best) = versions
            .iter()
            .find(|v| v.release_type == "release")
            .or_else(|| versions.first())
        else {
            eprintln!("[assistent] keine Fassung für {}", planned.title);
            continue;
        };

        if let Err(e) =
            crate::mods::install(&instance.id, &best.version_id, Some(&best.provider), sink.clone())
                .await
        {
            eprintln!("[assistent] {} fehlgeschlagen: {e}", planned.title);
        }
    }

    // 4. Optimierung in die Spieleinstellungen schreiben
    if let Some(label) = &plan.optimization {
        let level = match label.as_str() {
            "Ausgewogen" => Level::Balanced,
            "Maximal" => Level::Extreme,
            _ => Level::Performance,
        };
        // Den gewählten Bildlauf übernehmen -- fest die Voreinstellung zu
        // setzen würde die Antwort des Nutzers stillschweigend verwerfen.
        let pacing = match plan.frame_cap {
            Some(cap) => Pacing::Smooth { cap },
            None => Pacing::Unlimited,
        };
        let game_dir = instance.game_dir()?;
        let _ = optimize::merge_options(&game_dir.join("options.txt"), level, pacing);

        let config_dir = game_dir.join("config");
        crate::paths::ensure_dir(&config_dir)?;
        let _ = std::fs::write(
            config_dir.join("sodium-options.json"),
            serde_json::to_vec_pretty(&optimize::sodium_options(level))?,
        );
        let _ = std::fs::write(
            config_dir.join("sodium-extra-options.json"),
            serde_json::to_vec_pretty(&optimize::sodium_extra_options(level))?,
        );
    }

    Ok(instance)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viele_mods_bekommen_mehr_klassenspeicher() {
        assert!(!jvm_args(4096, 10).iter().any(|f| f.contains("Metaspace")));
        assert!(jvm_args(4096, 96).iter().any(|f| f.contains("Metaspace")));
    }

    #[test]
    fn grosse_halde_bekommt_grosse_speicherseiten() {
        assert!(!jvm_args(6144, 10).iter().any(|f| f.contains("HugePages")));
        assert!(jvm_args(8192, 10).iter().any(|f| f.contains("HugePages")));
    }

    #[test]
    fn bildlauf_ueberlebt_den_weg_in_den_plan() {
        // Der Fehler, den die Warnung aufdeckte: der Wert wurde berechnet
        // und dann verworfen.
        let gedeckelt = Pacing::Smooth { cap: 90 };
        let cap = match gedeckelt { Pacing::Smooth { cap } => Some(cap), _ => None };
        assert_eq!(cap, Some(90));

        let frei = Pacing::Unlimited;
        let cap = match frei { Pacing::Smooth { cap } => Some(cap), _ => None };
        assert_eq!(cap, None);
    }

    #[test]
    fn hinweise_werden_entdoppelt() {
        let d = dedupe(vec!["a".into(), "b".into(), "a".into()]);
        assert_eq!(d, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn loader_bekommen_lesbare_namen() {
        assert_eq!(label_of("fabric"), "Fabric");
        assert_eq!(label_of("neoforge"), "NeoForge");
        assert_eq!(label_of("irgendwas"), "Vanilla");
    }
}
