//! Eine Installation planen, bevor etwas geschrieben wird.
//!
//! Mods setzen einander voraus. Wer Sodium Extra ohne Sodium installiert,
//! bekommt beim Start eine Meldung, die nach einem Fehler des Spiels
//! aussieht. Deshalb wird erst geprueft, was mitkommen muss, was schon da
//! ist und was sich stoesst -- und das Ergebnis vorgelegt, statt es
//! stillschweigend zu tun.

use std::collections::HashSet;

use serde::Serialize;

use crate::error::Result;
use crate::modrinth::{self, ModVersion};

#[derive(Debug, Clone, Serialize)]
pub struct ResolveConflict {
    /// `incompatible`, `duplicate`, `wrong_loader`, `wrong_game_version`
    pub kind: String,
    /// Worum es geht -- Titel oder Dateiname.
    pub subject: String,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResolvePlan {
    /// Was installiert wuerde, in der Reihenfolge der Abhaengigkeit:
    /// Voraussetzungen zuerst.
    pub install: Vec<ModVersion>,
    /// Bereits vorhandene Projekte, die uebersprungen werden.
    pub already_present: Vec<String>,
    pub conflicts: Vec<ResolveConflict>,
    pub total_bytes: u64,
}

/// Wie tief Abhaengigkeiten verfolgt werden.
///
/// In der Praxis sind es ein bis zwei Ebenen. Die Grenze schuetzt vor
/// Ringen und davor, dass ein Fehler im Portal zu endlosem Nachladen fuehrt.
const MAX_DEPTH: usize = 4;

/// Stellt den Plan zusammen.
pub async fn resolve(
    instance_id: &str,
    version_id: &str,
    game_version: &str,
    loader: &str,
) -> Result<ResolvePlan> {
    let installed = super::list(instance_id).unwrap_or_default();
    let present: HashSet<String> = installed
        .iter()
        .filter_map(|m| m.project_id.clone())
        .collect();

    let root = modrinth::version(version_id).await?;

    let mut plan = ResolvePlan {
        install: Vec::new(),
        already_present: Vec::new(),
        conflicts: Vec::new(),
        total_bytes: 0,
    };

    let mut seen: HashSet<String> = HashSet::new();
    collect(
        root, game_version, loader, &present, &mut seen, &mut plan, 0,
    )
    .await;

    plan.total_bytes = plan.install.iter().map(|v| v.size_bytes).sum();
    Ok(plan)
}

/// Passt die Fassung ueberhaupt zu dieser Instanz?
fn check_fit(
    version: &ModVersion,
    game_version: &str,
    loader: &str,
    conflicts: &mut Vec<ResolveConflict>,
) {
    if !version.game_versions.is_empty()
        && !version.game_versions.iter().any(|v| v == game_version)
    {
        conflicts.push(ResolveConflict {
            kind: "wrong_game_version".into(),
            subject: version.name.clone(),
            detail: format!(
                "Diese Fassung ist nicht für Minecraft {game_version} gebaut, \
                 sondern für {}.",
                version.game_versions.join(", ")
            ),
        });
    }

    let loader = modrinth::mod_loader(loader);
    let checkable = ["fabric", "forge", "neoforge", "quilt"].contains(&loader);
    if checkable && !version.loaders.is_empty() && !version.loaders.iter().any(|l| l == loader) {
        conflicts.push(ResolveConflict {
            kind: "wrong_loader".into(),
            subject: version.name.clone(),
            detail: format!(
                "Diese Mod ist für {} gebaut, die Instanz nutzt {loader}.",
                version.loaders.join(", ")
            ),
        });
    }
}

/// Letzte Prüfung mit den gespeicherten Instanzdaten, vor jedem Download.
pub(crate) fn ensure_fit(version: &ModVersion, game_version: &str, loader: &str) -> Result<()> {
    let mut conflicts = Vec::new();
    check_fit(version, game_version, loader, &mut conflicts);
    if let Some(conflict) = conflicts.first() {
        return Err(crate::Error::other(conflict.detail.clone()));
    }
    Ok(())
}

/// Sammelt rekursiv, was installiert werden muss.
///
/// Als Box, weil eine asynchrone Funktion sich sonst nicht selbst
/// aufrufen kann -- ihre Groesse waere nicht bestimmbar.
fn collect<'a>(
    version: ModVersion,
    game_version: &'a str,
    loader: &'a str,
    present: &'a HashSet<String>,
    seen: &'a mut HashSet<String>,
    plan: &'a mut ResolvePlan,
    depth: usize,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        if depth > MAX_DEPTH || !seen.insert(version.project_id.clone()) {
            return;
        }

        if present.contains(&version.project_id) {
            plan.already_present.push(version.name.clone());
            return;
        }

        // Auch fest referenzierte Abhängigkeiten können den falschen Loader haben.
        check_fit(&version, game_version, loader, &mut plan.conflicts);

        // Erst die Voraussetzungen, damit sie im Plan vorne stehen.
        for dependency in version.dependencies.clone() {
            match dependency.kind.as_str() {
                "required" => {
                    if let Some(needed) = fetch_dependency(
                        &dependency.version_id,
                        &dependency.project_id,
                        game_version,
                        loader,
                    )
                    .await
                    {
                        collect(needed, game_version, loader, present, seen, plan, depth + 1)
                            .await;
                    } else if let Some(id) = &dependency.project_id {
                        plan.conflicts.push(ResolveConflict {
                            kind: "incompatible".into(),
                            subject: version.name.clone(),
                            detail: format!(
                                "Benötigt ein Projekt ({id}), für das es keine Fassung \
                                 für Minecraft {game_version} gibt."
                            ),
                        });
                    }
                }
                "incompatible" => {
                    if let Some(id) = &dependency.project_id {
                        if present.contains(id) {
                            plan.conflicts.push(ResolveConflict {
                                kind: "incompatible".into(),
                                subject: version.name.clone(),
                                detail: format!(
                                    "Verträgt sich nicht mit einer bereits \
                                     installierten Mod ({id})."
                                ),
                            });
                        }
                    }
                }
                // `optional` und `embedded` brauchen nichts.
                _ => {}
            }
        }

        plan.install.push(version);
    })
}

/// Holt eine Abhaengigkeit -- entweder die genannte Fassung oder die
/// neueste passende des Projekts.
async fn fetch_dependency(
    version_id: &Option<String>,
    project_id: &Option<String>,
    game_version: &str,
    loader: &str,
) -> Option<ModVersion> {
    if let Some(id) = version_id {
        if let Ok(v) = modrinth::version(id).await {
            return Some(v);
        }
    }
    let project = project_id.as_ref()?;
    let available = modrinth::versions(project, Some(game_version), Some(loader))
        .await
        .ok()?;
    available
        .iter()
        .find(|v| v.release_type == "release")
        .or_else(|| available.first())
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modrinth::ModDependency;

    fn version(name: &str, project: &str, games: &[&str], loaders: &[&str]) -> ModVersion {
        ModVersion {
            version_id: format!("v-{project}"),
            project_id: project.into(),
            provider: "modrinth".into(),
            name: name.into(),
            version_number: "1.0".into(),
            changelog: None,
            release_type: "release".into(),
            downloads: 0,
            published_at: String::new(),
            game_versions: games.iter().map(|s| s.to_string()).collect(),
            loaders: loaders.iter().map(|s| s.to_string()).collect(),
            filename: format!("{project}.jar"),
            url: String::new(),
            size_bytes: 100,
            sha1: None,
            dependencies: vec![],
        }
    }

    #[test]
    fn falsche_minecraft_version_wird_erkannt() {
        let mut c = Vec::new();
        check_fit(&version("Sodium", "s", &["1.20.1"], &["fabric"]), "1.21.4", "fabric", &mut c);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, "wrong_game_version");
    }

    #[test]
    fn falscher_loader_wird_erkannt() {
        let mut c = Vec::new();
        check_fit(&version("X", "x", &["1.21.4"], &["forge"]), "1.21.4", "fabric", &mut c);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].kind, "wrong_loader");
    }

    #[test]
    fn vanilla_wird_nicht_gegen_loader_geprueft() {
        let mut c = Vec::new();
        check_fit(&version("X", "x", &["1.21.4"], &["fabric"]), "1.21.4", "vanilla", &mut c);
        assert!(c.is_empty(), "sonst wäre jede Mod ein Konflikt");
    }

    #[test]
    fn iris_neoforge_wird_in_buildmc_abgewiesen() {
        let iris = version("Iris", "iris", &["1.21.4"], &["neoforge"]);
        assert!(ensure_fit(&iris, "1.21.4", "buildmc").is_err());
        assert!(ensure_fit(&iris, "1.21.4", "fabric").is_err());
        assert!(ensure_fit(&iris, "1.21.4", "neoforge").is_ok());
        assert!(!modrinth::matches_target(&iris, Some("1.21.4"), Some("buildmc")));
    }

    #[test]
    fn loader_und_spielversion_muessen_zusammen_passen() {
        for target in ["fabric", "quilt", "forge", "neoforge", "buildmc"] {
            for candidate in ["fabric", "quilt", "forge", "neoforge"] {
                let v = version("Mod", "m", &["1.21.4"], &[candidate]);
                let expected = modrinth::mod_loader(target) == candidate;
                assert_eq!(ensure_fit(&v, "1.21.4", target).is_ok(), expected);
                assert_eq!(modrinth::matches_target(&v, Some("1.21.4"), Some(target)), expected);
                assert!(ensure_fit(&v, "1.20.1", target).is_err());
            }
        }
    }

    #[test]
    fn feste_abhaengigkeit_mit_falschem_loader_wird_erkannt() {
        let mut plan = ResolvePlan {
            install: vec![], already_present: vec![], conflicts: vec![], total_bytes: 0,
        };
        futures::executor::block_on(collect(
            version("Dependency", "dep", &["1.21.4"], &["neoforge"]),
            "1.21.4", "buildmc", &HashSet::new(), &mut HashSet::new(), &mut plan, 1,
        ));
        assert_eq!(plan.conflicts[0].kind, "wrong_loader");
    }

    #[test]
    fn passende_fassung_ergibt_keinen_konflikt() {
        let mut c = Vec::new();
        check_fit(&version("S", "s", &["1.21.4"], &["fabric"]), "1.21.4", "fabric", &mut c);
        assert!(c.is_empty());
    }

    #[test]
    fn unvertraegliche_mod_wird_gemeldet() {
        let mut v = version("A", "a", &["1.21.4"], &["fabric"]);
        v.dependencies.push(ModDependency {
            project_id: Some("b".into()),
            version_id: None,
            kind: "incompatible".into(),
        });

        let present: HashSet<String> = ["b".to_string()].into_iter().collect();
        let mut plan = ResolvePlan {
            install: vec![], already_present: vec![], conflicts: vec![], total_bytes: 0,
        };
        let mut seen = HashSet::new();

        // Direkt der Sammler, ohne Netz -- die Abhaengigkeit ist bereits da.
        futures::executor::block_on(collect(
            v, "1.21.4", "fabric", &present, &mut seen, &mut plan, 0,
        ));

        assert_eq!(plan.conflicts.len(), 1);
        assert_eq!(plan.conflicts[0].kind, "incompatible");
    }

    #[test]
    fn bereits_vorhandenes_wird_uebersprungen() {
        let v = version("Sodium", "s", &["1.21.4"], &["fabric"]);
        let present: HashSet<String> = ["s".to_string()].into_iter().collect();
        let mut plan = ResolvePlan {
            install: vec![], already_present: vec![], conflicts: vec![], total_bytes: 0,
        };
        let mut seen = HashSet::new();

        futures::executor::block_on(collect(
            v, "1.21.4", "fabric", &present, &mut seen, &mut plan, 0,
        ));

        assert!(plan.install.is_empty());
        assert_eq!(plan.already_present, vec!["Sodium"]);
    }
}
