#!/usr/bin/env node
// Verhindert genau den Fehler, der schon einmal passiert ist: eine
// manuell hochgezaehlte Version in einer Datei, waehrend die anderen
// zwei zurueckbleiben. `npm run build` liest package.json, Tauri
// tauri.conf.json, und CARGO_PKG_VERSION treibt die eigene
// Update-Pruefung -- klaffen sie auseinander, baut man scheinbar neu
// und bekommt trotzdem die alte Version angezeigt.
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");

const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8")).version;
const tauri = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8")).version;

const cargoText = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
const cargoMatch = cargoText.match(/^version\s*=\s*"([^"]+)"/m);
const cargo = cargoMatch?.[1];

const stellen = { "package.json": pkg, "tauri.conf.json": tauri, "Cargo.toml": cargo };
const werte = new Set(Object.values(stellen));

if (werte.size !== 1) {
  console.error("\n\x1b[31mVersionsangaben stimmen nicht überein:\x1b[0m");
  for (const [datei, wert] of Object.entries(stellen)) {
    console.error(`  ${datei.padEnd(18)} ${wert ?? "(nicht gefunden)"}`);
  }
  console.error("\nAlle drei müssen gleich sein, sonst zeigt der Build die falsche Version.\n");
  process.exit(1);
}

console.log(`Version stimmt überein: ${pkg}`);
