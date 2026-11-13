/* =========================================================================
   Versionskanaele.

   Das Mojang-Manifest kennt nur vier Typen: release, snapshot, old_beta,
   old_alpha. Pre-Releases, Release Candidates und die Aprilscherz-Bauten
   liegen alle unter "snapshot" und sind ausschliesslich am Namen zu
   unterscheiden. Diese Ableitung ist damit reine Zeichenkettenarbeit --
   und deshalb hier isoliert und ohne Seiteneffekt, damit sie pruefbar ist.
   ========================================================================= */

import type { ManifestType, VersionChannel, VersionEntry } from "./types";

/** Normale Schnappschuesse: 24w14a, 23w13a, 16w20a ... */
const SNAPSHOT_RE = /^\d{2}w\d{2}[a-z]$/i;

/** Pre-Release: 1.21.4-pre1, 1.14.4-pre7, auch "Pre-Release 3" alt. */
const PRE_RE = /-pre\d*$|[ -]pre[- ]?release[ ]?\d*$/i;

/** Release Candidate: 1.21.4-rc1, 1.19-rc2. */
const RC_RE = /-rc\d*$/i;

/** Sonderbauten: 23w13a_or_b, 20w14infinite, 3D Shareware, 15w14a (Love&Hugs),
    1.RV-Pre1 (Trendy Update), 2.0 (Aprilscherz), 22w13oneblockatatime. */
const EXPERIMENTAL_RE =
  /_or_b|infinite|shareware|combat[- ]?test|1\.RV|^2\.0$|oneblockatatime|potato/i;

/**
 * Leitet aus Manifest-Typ und Versions-Id den echten Kanal ab.
 * Die Reihenfolge der Pruefungen ist bedeutsam: Sonderbauten zuerst,
 * weil etwa "23w13a_or_b" sonst als normaler Schnappschuss durchgeht.
 */
export function channelOf(id: string, kind: ManifestType): VersionChannel {
  if (kind === "old_beta") return "old_beta";
  if (kind === "old_alpha") return "old_alpha";

  if (EXPERIMENTAL_RE.test(id)) return "experimental";
  if (kind === "release") return "release";

  if (RC_RE.test(id)) return "release_candidate";
  if (PRE_RE.test(id)) return "pre_release";
  if (SNAPSHOT_RE.test(id)) return "snapshot";

  /* Unter "snapshot" gemeldet, aber in keinem bekannten Muster: als
     Schnappschuss fuehren statt zu verwerfen. */
  return "snapshot";
}

/** Ergaenzt eine Manifest-Zeile um ihren Kanal. */
export function withChannel(e: Omit<VersionEntry, "channel">): VersionEntry {
  return { ...e, channel: channelOf(e.id, e.kind) };
}

export const CHANNELS: VersionChannel[] = [
  "release", "release_candidate", "pre_release",
  "snapshot", "experimental", "old_beta", "old_alpha",
];

export const CHANNEL_LABEL: Record<VersionChannel, string> = {
  release:           "Release",
  release_candidate: "Release Candidate",
  pre_release:       "Pre-Release",
  snapshot:          "Snapshot",
  experimental:      "Experimentell",
  old_beta:          "Beta (alt)",
  old_alpha:         "Alpha (alt)",
};

/** Kurzform fuer Abzeichen, wo der Platz knapp ist. */
export const CHANNEL_SHORT: Record<VersionChannel, string> = {
  release: "Rel", release_candidate: "RC", pre_release: "Pre",
  snapshot: "Snap", experimental: "Exp", old_beta: "Beta", old_alpha: "Alpha",
};

/** Farbkanal je Kanal, als HSL-Farbton fuer die Tokens. */
export const CHANNEL_HUE: Record<VersionChannel, number> = {
  release: 152, release_candidate: 190, pre_release: 205,
  snapshot: 38, experimental: 300, old_beta: 28, old_alpha: 18,
};

/** Wie stabil ist der Kanal? Steuert Warnhinweise beim Anlegen. */
export function isStable(c: VersionChannel): boolean {
  return c === "release";
}

/** Voreinstellung: nur Releases zeigen. Alles andere schaltet der Nutzer zu. */
export const DEFAULT_CHANNELS: VersionChannel[] = ["release"];
