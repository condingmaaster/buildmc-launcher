/* Belegt die Kanal-Ableitung an echten Versions-Ids aus dem Manifest.
   Aufruf: node --test (nach tsc) -- oder als Referenzliste beim Lesen. */

import { channelOf } from "./versions";

export const CASES: [string, Parameters<typeof channelOf>[1], string][] = [
  ["1.21.4",               "release",   "release"],
  ["26.1.2",               "release",   "release"],
  ["24w14a",               "snapshot",  "snapshot"],
  ["1.21.4-pre1",          "snapshot",  "pre_release"],
  ["1.21.4-rc1",           "snapshot",  "release_candidate"],
  ["23w13a_or_b",          "snapshot",  "experimental"],
  ["20w14infinite",        "snapshot",  "experimental"],
  ["1.RV-Pre1",            "snapshot",  "experimental"],
  ["3D Shareware v1.34",   "snapshot",  "experimental"],
  ["22w13oneblockatatime", "snapshot",  "experimental"],
  ["combat-test-8c",       "snapshot",  "experimental"],
  ["b1.7.3",               "old_beta",  "old_beta"],
  ["a1.2.6",               "old_alpha", "old_alpha"],
  ["inf-20100618",         "old_alpha", "old_alpha"],
];

export function check(): string[] {
  const bad: string[] = [];
  for (const [id, kind, want] of CASES) {
    const got = channelOf(id, kind);
    if (got !== want) bad.push(`${id}: erwartet ${want}, erhalten ${got}`);
  }
  return bad;
}
