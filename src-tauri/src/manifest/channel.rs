//! Ableitung des Versionskanals.
//!
//! Das Manifest kennt nur vier Typen. Pre-Releases, Release Candidates und
//! die Sonderbauten liegen alle unter `snapshot` und sind nur am Namen zu
//! unterscheiden. Diese Ableitung entspricht Zeile fuer Zeile der Fassung
//! in `src/lib/versions.ts` -- beide Seiten muessen dasselbe Ergebnis
//! liefern, sonst widersprechen sich Filter und Anzeige.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VersionChannel {
    Release,
    ReleaseCandidate,
    PreRelease,
    Snapshot,
    Experimental,
    OldBeta,
    OldAlpha,
}

/// Normale Schnappschuesse: `24w14a`, `16w20a`.
fn is_snapshot_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 6
        && b[0].is_ascii_digit()
        && b[1].is_ascii_digit()
        && (b[2] == b'w' || b[2] == b'W')
        && b[3].is_ascii_digit()
        && b[4].is_ascii_digit()
        && b[5].is_ascii_alphabetic()
}

fn is_rc(id: &str) -> bool {
    split_suffix(id, "-rc")
}

fn is_pre(id: &str) -> bool {
    split_suffix(id, "-pre")
}

/// Prueft auf `<marke><ziffern>` am Ende, ohne Beachtung der Gross-/Kleinschreibung.
fn split_suffix(id: &str, marker: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    match lower.rfind(marker) {
        Some(at) => lower[at + marker.len()..].chars().all(|c| c.is_ascii_digit()),
        None => false,
    }
}

const EXPERIMENTAL_MARKERS: [&str; 8] = [
    "_or_b",
    "infinite",
    "shareware",
    "combat-test",
    "combat test",
    "1.rv",
    "oneblockatatime",
    "potato",
];

fn is_experimental(id: &str) -> bool {
    let lower = id.to_ascii_lowercase();
    lower == "2.0" || EXPERIMENTAL_MARKERS.iter().any(|m| lower.contains(m))
}

/// Die Reihenfolge ist bedeutsam: Sonderbauten zuerst, sonst geht
/// `23w13a_or_b` als gewoehnlicher Schnappschuss durch.
pub fn channel_of(id: &str, kind: &str) -> VersionChannel {
    match kind {
        "old_beta" => return VersionChannel::OldBeta,
        "old_alpha" => return VersionChannel::OldAlpha,
        _ => {}
    }

    if is_experimental(id) {
        return VersionChannel::Experimental;
    }
    if kind == "release" {
        return VersionChannel::Release;
    }
    if is_rc(id) {
        return VersionChannel::ReleaseCandidate;
    }
    if is_pre(id) {
        return VersionChannel::PreRelease;
    }
    if is_snapshot_id(id) {
        return VersionChannel::Snapshot;
    }

    // Als Schnappschuss fuehren statt verwerfen.
    VersionChannel::Snapshot
}

#[cfg(test)]
mod tests {
    use super::{channel_of, VersionChannel as C};

    /// Dieselben Faelle wie in `src/lib/versions.test.ts`.
    #[test]
    fn kanaele() {
        let cases: &[(&str, &str, C)] = &[
            ("1.21.4", "release", C::Release),
            ("26.1.2", "release", C::Release),
            ("24w14a", "snapshot", C::Snapshot),
            ("15w14a", "snapshot", C::Snapshot),
            ("1.21.4-pre1", "snapshot", C::PreRelease),
            ("1.14.4-pre7", "snapshot", C::PreRelease),
            ("1.21.4-rc1", "snapshot", C::ReleaseCandidate),
            ("1.19-rc2", "snapshot", C::ReleaseCandidate),
            ("23w13a_or_b", "snapshot", C::Experimental),
            ("20w14infinite", "snapshot", C::Experimental),
            ("1.RV-Pre1", "snapshot", C::Experimental),
            ("3D Shareware v1.34", "snapshot", C::Experimental),
            ("22w13oneblockatatime", "snapshot", C::Experimental),
            ("combat-test-8c", "snapshot", C::Experimental),
            ("2.0", "snapshot", C::Experimental),
            ("b1.7.3", "old_beta", C::OldBeta),
            ("a1.2.6", "old_alpha", C::OldAlpha),
            ("inf-20100618", "old_alpha", C::OldAlpha),
        ];
        for (id, kind, want) in cases {
            assert_eq!(channel_of(id, kind), *want, "Version {id}");
        }
    }
}
