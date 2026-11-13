//! Java finden und pruefen.
//!
//! Minecraft stellt je Version eine Mindestanforderung (1.17 braucht 16,
//! ab 1.20.5 sind es 21). Ein zu altes Java scheitert erst beim Start mit
//! einer unverstaendlichen Meldung -- deshalb wird vorher geprueft.

pub mod install;

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JavaInfo {
    pub path: String,
    pub version: String,
    pub major: u32,
    pub vendor: String,
    pub arch: String,
    /// Vom Launcher selbst verwaltet (unter `jdks/`).
    pub is_bundled: bool,
}

fn exe_name() -> &'static str {
    if cfg!(windows) { "java.exe" } else { "java" }
}

/// Fragt eine Java-Installation nach ihrer Fassung.
///
/// `java -XshowSettings:properties -version` schreibt die Eigenschaften
/// nach stderr -- das ist zuverlaessiger als die Versionszeile, weil es
/// auch Anbieter und Architektur nennt.
pub fn probe(java_path: &Path) -> Result<JavaInfo> {
    if !java_path.exists() {
        return Err(Error::not_found(format!("{} existiert nicht", java_path.display())));
    }

    let out = Command::new(java_path)
        .args(["-XshowSettings:properties", "-version"])
        .output()
        .map_err(|e| Error::other(format!("{} nicht ausführbar: {e}", java_path.display())))?;

    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );

    let version = property(&text, "java.version")
        .ok_or_else(|| Error::other("Java meldet keine Version"))?;
    let vendor = property(&text, "java.vendor").unwrap_or_else(|| "unbekannt".into());
    let arch = property(&text, "os.arch").unwrap_or_else(|| "unbekannt".into());

    Ok(JavaInfo {
        major: major_of(&version),
        path: java_path.display().to_string(),
        version,
        vendor,
        arch,
        is_bundled: paths::jdks().map(|d| java_path.starts_with(d)).unwrap_or(false),
    })
}

fn property(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find_map(|l| l.strip_prefix(key)?.trim().strip_prefix('=').map(|v| v.trim().to_string()))
}

/// `1.8.0_412` ergibt 8, `21.0.5` ergibt 21.
pub fn major_of(version: &str) -> u32 {
    let head = version.split(['.', '-', '_']).next().unwrap_or("");
    match head.parse::<u32>() {
        Ok(1) => version
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(8),
        Ok(n) => n,
        Err(_) => 0,
    }
}

/// Durchsucht die ueblichen Orte nach Java-Installationen.
pub fn discover() -> Vec<JavaInfo> {
    let mut found: Vec<JavaInfo> = Vec::new();
    let mut seen: Vec<PathBuf> = Vec::new();

    let consider = |candidate: PathBuf, out: &mut Vec<JavaInfo>, seen: &mut Vec<PathBuf>| {
        let real = candidate.canonicalize().unwrap_or(candidate);
        if seen.contains(&real) {
            return;
        }
        seen.push(real.clone());
        if let Ok(info) = probe(&real) {
            if info.major > 0 {
                out.push(info);
            }
        }
    };

    // Selbst verwaltete Installationen zuerst.
    if let Ok(dir) = paths::jdks() {
        for entry in read_dir(&dir) {
            consider(entry.join("bin").join(exe_name()), &mut found, &mut seen);
        }
    }

    // JAVA_HOME
    if let Ok(home) = std::env::var("JAVA_HOME") {
        consider(
            PathBuf::from(home).join("bin").join(exe_name()),
            &mut found,
            &mut seen,
        );
    }

    // Systemweite Ablagen
    for root in system_roots() {
        for entry in read_dir(&root) {
            // macOS schachtelt eine Ebene tiefer.
            for suffix in [
                PathBuf::from("bin").join(exe_name()),
                PathBuf::from("Contents").join("Home").join("bin").join(exe_name()),
            ] {
                consider(entry.join(&suffix), &mut found, &mut seen);
            }
        }
    }

    // Was im PATH liegt.
    if let Some(path) = std::env::var_os("PATH") {
        for dir in std::env::split_paths(&path) {
            consider(dir.join(exe_name()), &mut found, &mut seen);
        }
    }

    // Neueste zuerst -- die passt meistens.
    found.sort_by(|a, b| b.major.cmp(&a.major));
    found
}

fn read_dir(p: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(p)
        .map(|it| it.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default()
}

fn system_roots() -> Vec<PathBuf> {
    if cfg!(target_os = "windows") {
        vec![
            PathBuf::from(r"C:\Program Files\Java"),
            PathBuf::from(r"C:\Program Files\Eclipse Adoptium"),
            PathBuf::from(r"C:\Program Files\Microsoft\jdk"),
            PathBuf::from(r"C:\Program Files (x86)\Java"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Library/Java/JavaVirtualMachines")]
    } else {
        vec![
            PathBuf::from("/usr/lib/jvm"),
            PathBuf::from("/usr/java"),
            PathBuf::from("/opt/java"),
            PathBuf::from("/opt"),
        ]
    }
}

/// Waehlt die passende Installation fuer eine Mindestanforderung.
///
/// Bevorzugt wird die *kleinste* ausreichende Fassung: Minecraft laeuft
/// dort am ehesten wie getestet, und sehr neue Java-Fassungen brechen
/// gelegentlich aeltere Mods.
pub fn pick(required_major: u32, explicit: Option<&str>) -> Result<JavaInfo> {
    if let Some(p) = explicit.filter(|s| !s.trim().is_empty()) {
        return probe(Path::new(p));
    }

    let mut candidates: Vec<JavaInfo> = discover()
        .into_iter()
        .filter(|j| j.major >= required_major)
        .collect();

    candidates.sort_by_key(|j| j.major);
    candidates
        .into_iter()
        .next()
        .ok_or(Error::NoJava(required_major))
}

#[cfg(test)]
mod tests {
    use super::{major_of, property};

    #[test]
    fn hauptversion_aus_zeichenkette() {
        assert_eq!(major_of("1.8.0_412"), 8);
        assert_eq!(major_of("1.7.0"), 7);
        assert_eq!(major_of("17.0.9"), 17);
        assert_eq!(major_of("21"), 21);
        assert_eq!(major_of("25.0.1"), 25);
        assert_eq!(major_of("kaputt"), 0);
    }

    #[test]
    fn eigenschaften_aus_der_ausgabe() {
        let text = "    java.version = 21.0.5\n    java.vendor = Eclipse Adoptium\n    os.arch = amd64\n";
        assert_eq!(property(text, "java.version").as_deref(), Some("21.0.5"));
        assert_eq!(property(text, "java.vendor").as_deref(), Some("Eclipse Adoptium"));
        assert_eq!(property(text, "os.arch").as_deref(), Some("amd64"));
        assert_eq!(property(text, "fehlt"), None);
    }
}
