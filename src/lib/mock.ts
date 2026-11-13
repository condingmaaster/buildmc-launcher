/* =========================================================================
   Mock-Backend.

   Bedient dieselbe Schnittstelle wie Rust, damit die Oberflaeche ohne
   Tauri im Browser lauft. Die Instanzen entsprechen einer echten
   Installation, damit Layout und Textlaengen unter realistischen Daten
   geprueft werden. Wird im Tauri-Build nie geladen.
   ========================================================================= */

import type {
  Account, Config, Instance, InstalledMod, JavaInfo, LanServer, ManifestType,
  ModVersion, ProjectType, RunningInstance, SavedServer, SearchResult, Snapshot,
  Telemetry, VersionEntry,
} from "./types";
import { withChannel } from "./versions";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

const inst = (
  id: string, name: string, mc: string, loader: Instance["loader"],
  mods: number, lastPlayed: string, group: string | null,
  playSeconds: number, launches: number,
): Instance => ({
  id, name, mc_version: mc, loader,
  fabric_version: loader === "fabric" ? "0.19.5" : null,
  buildmc_mod_version: loader === "buildmc" ? "1.4.2" : null,
  mods: Array.from({ length: mods }, (_, i) => `mod-${i}.jar`),
  resource_packs: [], shader_packs: [], jvm_args: [],
  min_ram: 2048, max_ram: 6144, use_custom_ui: false,
  created_at: "2026-05-02T10:00:00Z", last_played: lastPlayed,
  group, play_seconds: playSeconds, launch_count: launches,
  icon: null, accent: null, notes: "", locked: false, java_path_override: null,
});

const INSTANCES: Instance[] = [
  inst("0c205d46", "Grade So",  "1.21.4",  "buildmc", 42, "2026-09-27T18:12:00Z", "Survival",  201_600, 148),
  inst("156caedd", "1.21.11",   "1.21.11", "buildmc", 18, "2026-09-27T09:41:00Z", "Survival",   54_300,  61),
  inst("24de3c6e", "1.16.5",    "1.16.5",  "buildmc", 96, "2026-09-26T21:03:00Z", "Modpacks",  388_800, 302),
  inst("3de6c8fa", "test",      "1.21.11", "vanilla",  0, "2026-09-25T14:20:00Z", null,          3_600,   7),
  inst("c32f091a", "26.1.2",    "26.1.2",  "buildmc",  7, "2026-08-13T19:55:00Z", "Testing",    21_900,  33),
  inst("d4972690", "1.21.1",    "1.21.1",  "buildmc", 31, "2026-09-25T16:38:00Z", "Survival",  142_200,  97),
  inst("fd52a932", "1.21.4",    "1.21.4",  "vanilla",  0, "2026-07-01T11:02:00Z", null,          8_100,  12),
];

/* Echte Kontonamen, UUIDs und Skins aus der vorhandenen Installation,
   damit Namenslaengen, Abstaende und der Kopf-Ausschnitt unter realen
   Daten geprueft werden.

   Die Skin-Adresse kommt im Betrieb aus dem Minecraft-Profil:
   fuer das eigene Konto ueber `api.minecraftservices.com/minecraft/profile`
   mit dem Zugriffstoken, fuer fremde Spieler ohne Token ueber
   `sessionserver.mojang.com/session/minecraft/profile/<uuid>`.
   Die hier eingetragenen Adressen stammen genau daher. */
const ACCOUNTS: Account[] = [
  { id: "a1", username: "JUWAN2011", uuid: "694aa712-2e20-499e-ab33-0ada95d10400",
    account_type: "microsoft", active: true, expires_at: Date.now() + 3_600_000,
    offline: false,
    skin_url: "https://textures.minecraft.net/texture/"
            + "3d996abc69ea70a20442855e429bf44b45111f9818d0f8c46272e12d12bec218" },
  { id: "a2", username: "CodingMaster_", uuid: "be61331d-4121-4692-bfb6-2d598421a01c",
    account_type: "microsoft", active: false, expires_at: Date.now() + 3_600_000,
    offline: false, skin_url: null },
  { id: "a3", username: "Testspieler", uuid: "b6f4a9c2-0000-3000-8000-1f2e3d4c5b6a",
    account_type: "offline", active: false, expires_at: 0,
    offline: true, skin_url: null },
];

const SERVERS: SavedServer[] = [
  { id: "s1", name: "BuildMC Hauptserver", address: "play.buildmc.io", favorite: true,
    status: { online: true, players_online: 47, players_max: 120, version: "BuildMC 26.1.2",
              motd: "Willkommen zurück", latency_ms: 24, favicon: null } },
  { id: "s2", name: "CommunitySMP", address: "smp.example.net", favorite: false,
    status: { online: true, players_online: 8, players_max: 40, version: "Paper 1.21.4",
              motd: "Saison 3 läuft", latency_ms: 91, favicon: null } },
  { id: "s3", name: "Testserver", address: "localhost:25565", favorite: false,
    status: { online: false, players_online: 0, players_max: 0, version: "",
              motd: "", latency_ms: 0, favicon: null } },
];

/* Je Inhaltstyp eigene Eintraege -- sonst zeigt jeder Reiter dasselbe,
   und genau das war der Fehler. */
/** Titel, Beschreibung, Autor, Modrinth-Kennung. */
type Entry = [string, string, string, string?];

const CATALOG: Record<ProjectType, Entry[]> = {
  mod: [
    ["Sodium", "Rendering-Engine mit deutlich höheren Bildraten", "jellysquid3", "sodium"],
    ["Iris Shaders", "Shaderpack-Unterstützung, verträgt sich mit Sodium", "coderbot", "iris"],
    ["Lithium", "Optimiert die Server-Tick-Schleife ohne Verhaltensänderung", "jellysquid3", "lithium"],
    ["FerriteCore", "Senkt den Speicherverbrauch der Blockzustände", "malte0811", "ferrite-core"],
    ["ModMenu", "Listet installierte Mods und öffnet ihre Einstellungen", "Prospector", "modmenu"],
    ["Cloth Config", "Gemeinsame Konfigurationsoberfläche für viele Mods", "shedaniel", "cloth-config"],
    ["JEI", "Zeigt Rezepte und Verwendungen zu jedem Gegenstand", "mezz", "jei"],
    ["Create", "Mechanische Bauteile, Wellen und Automatisierung", "simibubi", "create"],
    ["Distant Horizons", "Sichtweite weit über die Chunk-Grenze hinaus", "jeseibel", "distanthorizons"],
    ["Entity Culling", "Überspringt das Zeichnen verdeckter Entitäten", "tr7zw", "entityculling"],
    ["Xaeros Minimap", "Minikarte mit Wegpunkten und Höhlenansicht", "xaero96", "xaeros-minimap"],
    ["Mod Loading Screen", "Zeigt den Fortschritt beim Laden der Mods", "Prospector"],
  ],
  modpack: [
    ["Fabulously Optimized", "Vanilla-Gefühl, deutlich höhere Bildraten", "robotkoer"],
    ["Better MC", "Großes Abenteuerpaket mit Strukturen und Gegnern", "Luna Pixel"],
    ["Cobblemon Official", "Sammelmonster-Paket auf Fabric-Basis", "Cobblemon"],
    ["Create: Above and Beyond", "Fortschrittsbaum rund um Create", "simibubi"],
    ["Prominence II RPG", "Rollenspiel-Paket mit Klassen und Quests", "Hypnotic"],
    ["Vault Hunters 3rd", "Gewölbe-Läufe mit Fähigkeiten und Beute", "Iskall85"],
  ],
  resourcepack: [
    ["Faithful 32x", "Vanilla-Optik in doppelter Auflösung", "Faithful Team"],
    ["Bare Bones", "Flächige Texturen ohne Rauschen", "RobotPants"],
    ["Stay True", "Bleibt nah am Original, wirkt nur weicher", "TBurn"],
    ["Mizunos 16 Craft", "Ruhige, japanisch anmutende Bauoptik", "Mizuno"],
    ["Motschens Better Leaves", "Laub als volle Büschel statt flacher Flächen", "Motschen"],
    ["Fresh Animations", "Belebte Bewegungsabläufe für alle Kreaturen", "FreshLX"],
  ],
  shader: [
    ["Complementary Reimagined", "Ausgewogen, läuft auch auf schwacher Grafik", "EminGT"],
    ["BSL Shaders", "Weiches Licht, kräftige Farben", "CaptTatsu"],
    ["Sildurs Vibrant", "Viele Stufen von sparsam bis üppig", "Sildur"],
    ["Rethinking Voxels", "Voxelbasierte Beleuchtung mit echten Schatten", "gri573"],
    ["Photon", "Kräftige Himmel und glaubwürdiger Dunst", "SixthSurge"],
    ["MakeUp Ultra Fast", "Auf Bildrate getrimmt, kaum Einbußen", "Rutherin"],
  ],
  datapack: [
    ["Terralith", "Über hundert neue Biome ohne Mod", "Stardust Labs"],
    ["Incendium", "Überarbeitetes Nether mit neuen Bauwerken", "Stardust Labs"],
    ["Nullscape", "Weitläufiges, ruhigeres Ende", "Stardust Labs"],
    ["Vanilla Tweaks", "Viele kleine Verbesserungen zum Zusammenstellen", "Vanilla Tweaks"],
    ["Double Shulker Shells", "Schulker lassen zwei Schalen fallen", "Vanilla Tweaks"],
  ],
};

/* Die Vorschau fragt die echte Modrinth-API ab -- oeffentlich, ohne
   Schluessel, und genau der Weg, den das Rust-Backend spaeter nimmt.
   Damit zeigt die Oberflaeche echte Projektsymbole, Beschreibungen und
   Downloadzahlen statt erfundener Platzhalter. Faellt das Netz aus,
   uebernimmt der eingebaute Katalog, damit die Gestaltungsarbeit
   offline weitergeht. */

const MODRINTH = "https://api.modrinth.com/v2";

interface Hit {
  project_id: string; slug: string; title: string; description: string;
  author: string; downloads: number; follows: number;
  icon_url: string | null; categories: string[]; project_type: string;
  versions: string[]; date_modified: string; license: string | null;
  client_side: string; server_side: string;
}

const LOADER_CATEGORIES = new Set(["fabric", "forge", "neoforge", "quilt"]);

function fromHit(h: Hit): SearchResult {
  return {
    provider: "modrinth",
    project_id: h.project_id,
    slug: h.slug,
    title: h.title,
    description: h.description,
    author: h.author,
    downloads: h.downloads,
    follows: h.follows,
    icon_url: h.icon_url || null,
    /* Modrinth mischt Loader unter die Kategorien -- hier wieder trennen. */
    categories: h.categories.filter((c) => !LOADER_CATEGORIES.has(c)),
    project_type: (h.project_type as ProjectType) ?? "mod",
    loaders: h.categories.filter((c) => LOADER_CATEGORIES.has(c)) as SearchResult["loaders"],
    game_versions: h.versions,
    updated_at: h.date_modified,
    license: h.license,
    client_side: h.client_side as SearchResult["client_side"],
    server_side: h.server_side as SearchResult["server_side"],
  };
}

async function searchModrinth(
  query: string, page: number, type: ProjectType,
  gameVersion: string | null, loader: string | null,
  categories: string[] = [],
): Promise<{ results: SearchResult[]; total: number } | null> {
  const facets: string[][] = [[`project_type:${type}`]];
  if (gameVersion) facets.push([`versions:${gameVersion}`]);
  /* Vanilla und der BuildMC-Client sind keine Modrinth-Loader. */
  if (loader && LOADER_CATEGORIES.has(loader)) facets.push([`categories:${loader}`]);
  /* Kategorien als eigene Gruppe: innerhalb ODER, zwischen den Gruppen UND.
     Entscheidend für brauchbare Treffer -- „optimization" als Volltext
     findet nur Mods mit dem Wort im Namen, als Kategorie die echten. */
  if (categories.length) facets.push(categories.map((c) => `categories:${c}`));

  /* Ohne Suchwort nach Beliebtheit ordnen. */
  const index = query.trim() ? "" : "&index=downloads";
  const url = `${MODRINTH}/search?query=${encodeURIComponent(query)}`
            + `&limit=20&offset=${page * 20}${index}`
            + `&facets=${encodeURIComponent(JSON.stringify(facets))}`;

  try {
    const res = await fetch(url, { headers: { Accept: "application/json" } });
    if (!res.ok) return null;
    const data = await res.json() as { hits: Hit[]; total_hits: number };
    return { results: data.hits.map(fromHit), total: data.total_hits };
  } catch {
    return null;                      // offline: der Katalog uebernimmt
  }
}

/** Ersatz ohne Netz. */
function fallbackResults(query: string, page: number, type: ProjectType): SearchResult[] {
  const q = query.trim().toLowerCase();
  return (CATALOG[type] ?? [])
    .filter(([n, d]) => !q || n.toLowerCase().includes(q) || d.toLowerCase().includes(q))
    .slice(page * 10, page * 10 + 10)
    .map(([title, description, author], i) => ({
      provider: i % 3 === 2 ? "curseforge" : "modrinth",
      project_id: `p-${title.toLowerCase().replace(/\W+/g, "-")}`,
      slug: title.toLowerCase().replace(/\W+/g, "-"),
      title, description, author,
      downloads: 1_200_000 - i * 87_000,
      follows: 42_000 - i * 2_900,
      icon_url: null,
      categories: ["optimization", "utility"].slice(0, (i % 2) + 1),
      project_type: type,
      loaders: ["fabric", "quilt"],
      game_versions: ["1.21.4", "1.21.11"],
      updated_at: "2026-09-20T12:00:00Z",
      license: "LGPL-3.0",
      client_side: "required",
      server_side: i % 2 ? "optional" : "unsupported",
    }));
}

/* Modrinth beantwortet mehrere Kennungen in einem Aufruf. Ein Aufruf je
   Mod waere bei 96 Eintraegen ein Sturm von Anfragen -- und die Liste
   soll sofort stehen, nicht nach und nach aufploppen. */
const iconCache = new Map<string, string | null>();

async function loadIcons(slugs: string[]): Promise<void> {
  const missing = slugs.filter((s) => s && !iconCache.has(s));
  if (!missing.length) return;
  try {
    const ids = encodeURIComponent(JSON.stringify(missing));
    const res = await fetch(`${MODRINTH}/projects?ids=${ids}`);
    if (!res.ok) throw new Error(String(res.status));
    for (const p of await res.json() as { slug: string; icon_url: string | null }[]) {
      iconCache.set(p.slug, p.icon_url || null);
    }
  } catch {
    /* Offline: die Eintraege bekommen das erzeugte Ersatzbild. */
  }
  // Auch Fehlschlaege merken, damit nicht bei jedem Aufruf erneut gefragt wird.
  for (const s of missing) if (!iconCache.has(s)) iconCache.set(s, null);
}

function installed(n: number): InstalledMod[] {
  return CATALOG.mod.slice(0, n).map(([name, , , slug], i) => ({
    filename: `${name.toLowerCase().replace(/\W+/g, "-")}-1.4.${i}.jar`,
    name, version: `1.4.${i}`,
    enabled: i !== 3,
    provider: i % 3 === 2 ? "curseforge" : "modrinth",
    project_id: slug ?? `p-${i}`, version_id: `v-${i}`,
    icon_url: slug ? iconCache.get(slug) ?? null : null,
    update_available: i === 1 || i === 5 ? `1.5.${i}` : null,
    depends_on: i === 1 ? ["sodium-1.4.0.jar"] : [],
    size_bytes: 420_000 + i * 81_000,
  }));
}

const CONFIG: Config = {
  theme: "dark", accent_color: "#7c5cff", background_shade: 8,
  min_ram: 2048, max_ram: 6144, java_path: "", use_zgc: true,
  concurrent_downloads: 8, verify_sha1: true, close_on_launch: false,
  auto_update: true, skipped_update_version: null, discord_rpc: true,
  performance_mode: false, high_process_priority: false,
  easter_eggs_enabled: true, easter_egg_frequency: 3,
  ai_reasoning: true, ai_tools: true,
  language: "de", auto_snapshot: true, snapshot_keep: 5,
  mod_update_check: true, curseforge_enabled: true, telemetry_overlay: true,
};

/* Ein Querschnitt durch alle sieben Kanaele, damit die Filter und die
   Warnhinweise fuer instabile Versionen unter echten Ids gepruefbar sind. */
const RAW: [string, ManifestType, string, boolean, number][] = [
  ["26.2-rc1",             "snapshot",  "2026-09-22", false, 25],
  ["26.2-pre3",            "snapshot",  "2026-09-15", false, 25],
  ["26w38a",               "snapshot",  "2026-09-17", false, 25],
  ["26.1.2",               "release",   "2026-08-01", true,  25],
  ["26.1.1",               "release",   "2026-07-18", false, 25],
  ["26.1",                 "release",   "2026-07-02", false, 25],
  ["22w13oneblockatatime", "snapshot",  "2026-04-01", false, 21],
  ["1.21.11",              "release",   "2026-06-12", true,  21],
  ["1.21.4",               "release",   "2025-12-03", true,  21],
  ["1.21.4-rc1",           "snapshot",  "2025-11-28", false, 21],
  ["1.21.4-pre1",          "snapshot",  "2025-11-20", false, 21],
  ["1.21.1",               "release",   "2025-08-08", true,  21],
  ["23w13a_or_b",          "snapshot",  "2023-04-01", false, 17],
  ["1.20.1",               "release",   "2023-06-12", false, 17],
  ["20w14infinite",        "snapshot",  "2020-04-01", false, 8],
  ["1.16.5",               "release",   "2021-01-15", true,  8],
  ["1.RV-Pre1",            "snapshot",  "2016-04-01", false, 8],
  ["15w14a",               "snapshot",  "2015-04-01", false, 8],
  ["1.8.9",                "release",   "2015-12-09", false, 8],
  ["b1.7.3",               "old_beta",  "2011-07-08", false, 8],
  ["b1.5_01",              "old_beta",  "2011-04-19", false, 8],
  ["a1.2.6",               "old_alpha", "2011-01-13", false, 8],
  ["inf-20100618",         "old_alpha", "2010-06-18", false, 8],
];

const MANIFEST: VersionEntry[] = RAW.map(([id, kind, released_at, installed, java_major]) =>
  withChannel({
    id, kind, released_at, installed, java_major,
    available_loaders: kind === "release" && java_major >= 17
      ? ["vanilla", "buildmc", "fabric", "neoforge", "quilt"]
      : kind === "release" ? ["vanilla", "buildmc", "fabric", "forge"]
      : ["vanilla"],
  }),
);

const LOG_LINES: string[] = [
  "[21:40:58] [main/INFO]: Loading Minecraft 1.16.5 with Fabric Loader 0.19.5",
  "[21:40:59] [main/INFO]: Loading 96 mods:",
  "[21:41:02] [Render thread/INFO]: Setting user: Jona",
  "[21:41:04] [Render thread/WARN]: Unable to resolve texture reference: buildmc:wings",
  "[21:41:07] [Render thread/INFO]: OpenAL initialized on device Lautsprecher",
  "[21:41:11] [Render thread/ERROR]: Mixin apply for mod entity-culling failed",
  "[21:41:12] [Render thread/ERROR]: java.lang.NoSuchMethodError: net.minecraft.class_310.method_1551()",
  "[21:41:12] [Render thread/ERROR]: \tat tr7zw.entityculling.EntityCullingMod.onInit(EntityCullingMod.java:84)",
  "[21:41:12] [Render thread/ERROR]: \tat net.fabricmc.loader.impl.FabricLoaderImpl.invokeEntrypoints",
  "[21:41:12] [main/ERROR]: Minecraft wurde mit Code 1 beendet",
];

const DIAGNOSIS = {
  cause: "Mod passt nicht zur Minecraft-Version",
  explanation:
    "Entity Culling ruft eine Methode auf, die es in 1.16.5 nicht gibt. " +
    "Die installierte Fassung wurde für eine neuere Minecraft-Version gebaut. " +
    "Fabric lädt die Mod trotzdem, der Fehler fällt erst beim Zugriff auf.",
  suggestions: [
    "Entity Culling auf eine für 1.16.5 gebaute Fassung wechseln",
    "Die Mod vorübergehend abschalten und erneut starten",
    "Prüfen, ob weitere Mods für eine andere Version installiert sind",
  ],
  culprit_mod: "entity-culling",
};

/* Der Fragebogen, wie ihn das Backend liefert. */
const ASSIST_QUESTIONS: any[] = [
  { id: "version", prompt: "Welche Minecraft-Version?",
    why: "Bestimmt, welche Mods es überhaupt gibt.", multi: false,
    options: ["1.21.11", "1.21.4", "1.21.1", "1.16.5"].map((v, i) => ({
      id: v, label: v, detail: i === 0 ? "Neueste Ausgabe" : "", queries: [], project_type: null })) },
  { id: "loader", prompt: "Welcher Mod-Loader?",
    why: "Mods sind zwischen Loadern nicht austauschbar.", multi: false,
    options: [
      { id: "fabric", label: "Fabric", detail: "Die größte Auswahl an Mods, schlank und schnell aktuell.", queries: [], project_type: null },
      { id: "quilt", label: "Quilt", detail: "Abspaltung von Fabric, verträgt sich mit den meisten Fabric-Mods.", queries: [], project_type: null },
      { id: "vanilla", label: "Keiner", detail: "Reines Minecraft ohne Mods. Dann entfallen die Modfragen.", queries: [], project_type: null },
    ] },
  { id: "ziel", prompt: "Worum soll es in dieser Instanz gehen?",
    why: "Bestimmt, wonach gesucht wird. Mehreres ist möglich.", multi: true,
    options: [
      { id: "vanilla_plus", label: "Vanilla, nur besser", detail: "Spielgefühl bleibt, nur Bildrate und Bedienung werden besser.", queries: ["performance"], project_type: null },
      { id: "technik", label: "Technik und Automatisierung", detail: "Maschinen, Fließbänder, Energie.", queries: ["automation"], project_type: null },
      { id: "abenteuer", label: "Abenteuer und Erkundung", detail: "Neue Biome, Strukturen, Gegner, Aufgaben.", queries: ["adventure"], project_type: null },
      { id: "bauen", label: "Bauen und Gestalten", detail: "Mehr Blöcke, Formen und Werkzeuge zum Bauen.", queries: ["building"], project_type: null },
      { id: "magie", label: "Magie", detail: "Zauber, Rituale, Verzauberungen.", queries: ["magic"], project_type: null },
    ] },
  { id: "geraet", prompt: "Wie stark ist dein Rechner?",
    why: "Entscheidet über Sichtweite, Partikel und die Mod-Auswahl.", multi: false,
    options: [
      { id: "schwach", label: "Eher schwach", detail: "Ältere Grafik, wenig Arbeitsspeicher. Jede Bildrate zählt.", queries: [], project_type: null },
      { id: "mittel", label: "Durchschnittlich", detail: "Läuft normal, soll aber flüssig bleiben.", queries: [], project_type: null },
      { id: "stark", label: "Kräftig", detail: "Genug Reserven, Optik darf kosten.", queries: [], project_type: null },
    ] },
  { id: "optik", prompt: "Soll die Optik verändert werden?",
    why: "Shader und Texturen kosten spürbar Leistung.", multi: true,
    options: [
      { id: "shader", label: "Shader", detail: "Licht, Schatten, Wasserspiegelungen. Kostet am meisten.", queries: ["shaders"], project_type: "shader" },
      { id: "texturen", label: "Andere Texturen", detail: "Ein Resourcepack statt der Vanilla-Optik.", queries: ["faithful"], project_type: "resourcepack" },
      { id: "keine", label: "Nichts davon", detail: "Vanilla-Optik behalten.", queries: [], project_type: null },
    ] },
  { id: "komfort", prompt: "Welche Bequemlichkeiten willst du?",
    why: "Kleine Mods, die den Alltag angenehmer machen.", multi: true,
    options: [
      { id: "karte", label: "Minikarte und Wegpunkte", detail: "Orientierung ohne ständiges F3.", queries: ["minimap"], project_type: null },
      { id: "rezepte", label: "Rezepte nachschlagen", detail: "Zeigt, wie sich etwas herstellen lässt.", queries: ["recipe viewer"], project_type: null },
      { id: "inventar", label: "Inventar sortieren", detail: "Sortieren und Verschieben mit weniger Klicks.", queries: ["inventory management"], project_type: null },
      { id: "keine_extras", label: "Nichts davon", detail: "", queries: [], project_type: null },
    ] },
  { id: "server", prompt: "Willst du auf fremden Servern spielen?",
    why: "Manche Mods gelten dort als unerlaubt.", multi: false,
    options: [
      { id: "ja", label: "Ja, auch auf öffentlichen Servern", detail: "Du bekommst einen Hinweis, welche Mod-Arten dort häufig unerwünscht sind. Geprüft wird das nicht.", queries: [], project_type: null },
      { id: "nein", label: "Nur allein oder mit Freunden", detail: "Keine Einschränkung.", queries: [], project_type: null },
    ] },
  { id: "ram", prompt: "Wie viel Arbeitsspeicher soll die Instanz bekommen?",
    why: "Dein Rechner hat 32 GB. Mehr als die Hälfte zu geben bringt selten etwas.", multi: false,
    options: [
      { id: "2048", label: "2 GB — Vanilla und wenige Mods", detail: "", queries: [], project_type: null },
      { id: "4096", label: "4 GB — die übliche Wahl", detail: "", queries: [], project_type: null },
      { id: "6144", label: "6 GB — viele Mods oder große Sichtweite", detail: "", queries: [], project_type: null },
      { id: "8192", label: "8 GB — große Modpakete", detail: "", queries: [], project_type: null },
    ] },
];

let ASSIST: any = null;

let AI_STATE = {
  configured: false,
  key_hint: null as string | null,
  model: "mistral-medium-latest",
  from_env: false,
  rejected: [] as string[],
};

let RUNNING: RunningInstance[] = [
  { instance_id: "0c205d46", pid: 48211, started_at: new Date(Date.now() - 1_920_000).toISOString() },
];

let BEDROCK_INSTANCES = [
  { id: "bed-01", name: "Survival", version_dir_name: "1.21.50.07",
    created_at: "2026-08-01T10:00:00Z", last_played: "2026-09-26T20:00:00Z",
    play_seconds: 42_000, launch_count: 12 },
];
let BEDROCK_RUNNING: { instance_id: string; pid: number; started_at: string }[] = [];
const BEDROCK_VERSIONS = [{ dir_name: "1.21.50.07" }, { dir_name: "1.21.44.01" }];

/* --------------------------------------------------------------------- */

const handlers: Record<string, (a: any) => unknown> = {
  get_accounts: () => ACCOUNTS,
  list_instances: () => INSTANCES,
  list_trashed_instances: () => [],
  get_config: () => CONFIG,
  save_config: () => undefined,
  get_running_instances: () => RUNNING,

  launch_instance: ({ instanceId }: { instanceId: string }) => {
    if (!RUNNING.some((r) => r.instance_id === instanceId)) {
      RUNNING = [...RUNNING, {
        instance_id: instanceId,
        pid: 40000 + Math.floor(Math.random() * 9999),
        started_at: new Date().toISOString(),
      }];
    }
    return undefined;
  },

  stop_instance: ({ instanceId }: { instanceId: string }) => {
    RUNNING = RUNNING.filter((r) => r.instance_id !== instanceId);
    return undefined;
  },

  bedrock_status: () => ({
    platform_supported: true,
    client_found: true,
    client_path: "/usr/bin/mcpelauncher-client",
    ui_found: true,
    ui_path: "/usr/bin/mcpelauncher-ui-qt",
    google_account_linked: true,
    available_versions: BEDROCK_VERSIONS.length,
  }),
  bedrock_open_ui: () => undefined,
  bedrock_list_versions: () => BEDROCK_VERSIONS,
  bedrock_list_instances: () => BEDROCK_INSTANCES,
  bedrock_create_instance: ({ name, versionDirName }: { name: string; versionDirName: string }) => {
    const created = {
      id: `bed-${Math.random().toString(36).slice(2, 8)}`,
      name, version_dir_name: versionDirName,
      created_at: new Date().toISOString(), last_played: "",
      play_seconds: 0, launch_count: 0,
    };
    BEDROCK_INSTANCES = [...BEDROCK_INSTANCES, created];
    return created;
  },
  bedrock_start: ({ instanceId }: { instanceId: string }) => {
    if (!BEDROCK_RUNNING.some((r) => r.instance_id === instanceId)) {
      BEDROCK_RUNNING = [...BEDROCK_RUNNING, {
        instance_id: instanceId,
        pid: 50000 + Math.floor(Math.random() * 9999),
        started_at: new Date().toISOString(),
      }];
    }
    return { instance_id: instanceId, pid: 50000, started_at: new Date().toISOString() };
  },
  bedrock_stop: ({ instanceId }: { instanceId: string }) => {
    BEDROCK_RUNNING = BEDROCK_RUNNING.filter((r) => r.instance_id !== instanceId);
    return undefined;
  },
  bedrock_running: () => BEDROCK_RUNNING,

  list_servers: () => SERVERS,
  get_system_memory: () => 32 * 1024,

  list_instance_mods: async ({ instanceId }: { instanceId: string }) => {
    const n = INSTANCES.find((i) => i.id === instanceId)?.mods.length ?? 0;
    await loadIcons(CATALOG.mod.slice(0, n).map(([, , , slug]) => slug ?? ""));
    return installed(n);
  },

  search_mods: async ({ query, page, projectType, gameVersion, loader }: {
    query: string; page: number; projectType: ProjectType;
    gameVersion: string | null; loader: string | null;
  }) => {
    const type = projectType ?? "mod";
    const live = await searchModrinth(query ?? "", page ?? 0, type, gameVersion, loader);
    if (live) return live;
    return { results: fallbackResults(query ?? "", page ?? 0, type),
             total: (CATALOG[type] ?? []).length };
  },

  get_mod_versions: (): ModVersion[] => [{
    version_id: "v-1", provider: "modrinth", name: "Sodium 0.6.9",
    version_number: "0.6.9", changelog: "Behebt Flackern bei aktivem Nebel.",
    release_type: "release", downloads: 880_000, published_at: "2026-09-18T00:00:00Z",
    game_versions: ["1.21.4"], loaders: ["fabric"], filename: "sodium-0.6.9.jar",
    size_bytes: 912_000, sha1: null, dependencies: [],
  }],

  get_versions: (): VersionEntry[] => MANIFEST,

  detect_java: (): JavaInfo[] => [
    { path: "/usr/lib/jvm/java-21", version: "21.0.5", major: 21,
      vendor: "Temurin", arch: "x86_64", is_bundled: false },
  ],

  get_telemetry: ({ instanceId }: { instanceId: string }): Telemetry => ({
    instance_id: instanceId,
    rss_bytes: 4_180_000_000 + Math.random() * 3e8,
    max_bytes: 6_442_450_944,
    cpu_percent: 28 + Math.random() * 22,
    uptime_seconds: 1_920,
  }),

  list_snapshots: ({ instanceId }: { instanceId: string }): Snapshot[] => [
    { id: "snap-1", instance_id: instanceId, label: "Vor dem Mod-Update",
      created_at: "2026-09-26T20:14:00Z", size_bytes: 284_000_000,
      includes_worlds: true, automatic: true },
  ],

  get_launch_log: () => LOG_LINES,

  list_crash_reports: ({ instanceId }: { instanceId: string }) => [{
    instance_id: instanceId,
    occurred_at: "2026-09-26T21:41:12Z",
    exit_code: 1,
    signature: "java.lang.NoSuchMethodError: net.minecraft.class_310.method_1551()",
    diagnosis: DIAGNOSIS,
    log_excerpt: LOG_LINES.slice(-6).join("\n"),
  }],

  /* ---- Assistent ---- */
  assistant_start: ({ goal }: { goal: string }) => {
    ASSIST = {
      id: "sess-1",
      goal: goal ?? "",
      step: "answer",
      questions: ASSIST_QUESTIONS,
      answers: {},
      candidates: [],
      selected: [],
      plan: null,
      source: AI_STATE.configured ? "modell" : "katalog",
      created_at: new Date().toISOString(),
      notes: AI_STATE.configured
        ? ["Verstanden als: eine Instanz für Technik und Automatisierung."]
        : [],
    };
    return ASSIST;
  },
  assistant_load: () => ASSIST,
  assistant_open_sessions: () => (ASSIST && ASSIST.step !== "done" ? [ASSIST] : []),
  assistant_discard: () => { ASSIST = null; return undefined; },

  assistant_answer: ({ questionId, choices }: { questionId: string; choices: string[] }) => {
    if (!ASSIST) throw new Error("keine Sitzung");
    ASSIST = { ...ASSIST, answers: { ...ASSIST.answers, [questionId]: choices } };
    const gilt = (q: any) => !(ASSIST!.answers.loader?.includes("vanilla")
                               && ["ziel", "komfort"].includes(q.id));
    const offen = ASSIST.questions.filter(gilt).some((q: any) => !ASSIST!.answers[q.id]);
    if (!offen) ASSIST = { ...ASSIST, step: "choose" };
    return ASSIST;
  },

  assistant_candidates: async () => {
    if (!ASSIST) throw new Error("keine Sitzung");
    const version = ASSIST.answers.version?.[0] ?? "1.21.4";
    const loader = ASSIST.answers.loader?.[0] ?? "fabric";

    /* Aus den Antworten die Kategorien ableiten -- wie im Backend. */
    const KAT: Record<string, string[]> = {
      vanilla_plus: ["optimization", "utility"],
      technik: ["technology", "transportation"],
      abenteuer: ["adventure", "worldgen"],
      bauen: ["decoration"],
      magie: ["magic"],
    };
    const gewaehlteKats = (ASSIST.answers.ziel ?? []).flatMap((z: string) => KAT[z] ?? []);
    /* Die Optimierung kommt immer dazu — sie trägt die Bildrate. */
    const kats = [...new Set([...gewaehlteKats, "optimization"])];

    const live = await searchModrinth("", 0, "mod", version, loader, kats);
    const treffer = (live?.results ?? []).slice(0, 12);
    ASSIST = {
      ...ASSIST,
      candidates: treffer.map((r) => {
        /* Von wie vielen gewählten Kategorien wird der Treffer getragen? */
        const passend = kats.filter((k) => r.categories.includes(k));
        return {
          ...r,
          matched: passend.length ? passend : [kats[0]!],
          preselected: passend.length > 1 || r.downloads > 20_000_000,
        };
      }),
      selected: treffer
        .filter((r) => kats.filter((k) => r.categories.includes(k)).length > 1
                       || r.downloads > 20_000_000)
        .map((r) => r.project_id),
    };
    return ASSIST;
  },

  assistant_select: ({ projectIds }: { projectIds: string[] }) => {
    if (!ASSIST) throw new Error("keine Sitzung");
    ASSIST = { ...ASSIST, selected: projectIds };
    return ASSIST;
  },

  assistant_plan: ({ name }: { name?: string }) => {
    if (!ASSIST) throw new Error("keine Sitzung");
    const gewaehlt = ASSIST.candidates.filter((c: any) => ASSIST!.selected.includes(c.project_id));
    ASSIST = {
      ...ASSIST,
      step: "review",
      plan: {
        name: name || `${ASSIST.answers.version?.[0] ?? "1.21.4"} Fabric`,
        mc_version: ASSIST.answers.version?.[0] ?? "1.21.4",
        loader: (ASSIST.answers.loader?.[0] as any) ?? "fabric",
        loader_version: "0.19.5",
        min_ram: 2048,
        max_ram: Number(ASSIST.answers.ram?.[0] ?? 4096),
        optimization: ASSIST.answers.geraet?.[0] === "schwach" ? "Maximal" : "Leistung",
        frame_cap: 90,
        jvm_args: ["-XX:+UseZGC", "-XX:+AlwaysPreTouch"],
        mods: [
          ...gewaehlt.map((c: any) => ({
            project_id: c.project_id, title: c.title, icon_url: c.icon_url,
            reason: `Gefunden über: ${c.matched.join(", ")}`,
            from_optimization: false,
          })),
          { project_id: "sodium", title: "Rendering-Engine", icon_url: null,
            reason: "Ersetzt Minecrafts Renderer. Der größte Einzelgewinn überhaupt.",
            from_optimization: true },
          { project_id: "lithium", title: "Spiellogik", icon_url: null,
            reason: "Optimiert die Tick-Schleife, ohne das Spielverhalten zu ändern.",
            from_optimization: true },
        ],
        rationale: AI_STATE.configured
          ? "Die Zusammenstellung zielt auf flüssiges Spielen bei begrenzter Grafik. "
          + "Sodium und Lithium tragen den größten Teil, die übrigen Mods ergänzen "
          + "Bequemlichkeiten ohne nennenswerte Kosten."
          : null,
        notes: ASSIST.answers.server?.includes("ja")
          ? ["Für öffentliche Server: Minikarten mit Höhlenansicht gelten vielerorts als unerlaubt."]
          : [],
      },
    };
    return ASSIST;
  },

  assistant_apply: () => {
    if (ASSIST) ASSIST = { ...ASSIST, step: "done" };
    return INSTANCES[0];
  },

  ai_status: () => AI_STATE,
  ai_set_key: ({ key }: { key: string | null }) => {
    AI_STATE = { ...AI_STATE, configured: !!key,
                 key_hint: key ? `••••••••${key.slice(-4)}` : null };
    return AI_STATE;
  },
  ai_set_model: ({ model }: { model: string }) => {
    AI_STATE = { ...AI_STATE, model };
    return undefined;
  },
  /* Nur Modelle, die chatten können. Codestral, Pixtral und die
     Sonderzwecke wie mistral-embed stehen bewusst nicht zur Wahl — sie
     würden nur in unverständliche Fehler führen. */
  ai_list_models: () => [
    { id: "mistral-medium-latest", description: "Voreinstellung" },
    { id: "mistral-small-latest",  description: "Kleiner und schneller" },
    { id: "ministral-14b-latest",  description: "Großzügigere Anfragegrenze" },
    { id: "ministral-8b-latest",   description: "Schnell" },
    { id: "ministral-3b-latest",   description: "Sehr schnell" },
  ],
  ai_interpret_search: ({ wish }: { wish: string }) => ({
    queries: ["performance", "shaders", "optimization"],
    project_type: "mod",
    game_version: "1.21.4",
    loader: "fabric",
    reading: `Verstanden als: ${wish.trim() || "bessere Bildrate"} — gesucht wird nach `
           + "Leistungs- und Shader-Mods für 1.21.4 auf Fabric.",
  }),
  ai_explain_conflict: () => ({
    summary: "Zwei Mods verändern dieselbe Stelle im Rendering.",
    detail: "Sodium und OptiFine greifen beide tief in die Grafikausgabe ein "
          + "und ersetzen dort denselben Code. Zusammen installiert überschreibt "
          + "eine die Arbeit der anderen, was beim Laden der Welt abstürzt.",
    options: [
      "Bei Sodium bleiben und für Shader zusätzlich Iris installieren",
      "OptiFine entfernen, es wird für Fabric ohnehin kaum noch gepflegt",
    ],
  }),

  analyze_log: () => DIAGNOSIS,
  analyze_log_text: () => DIAGNOSIS,
  check_mod_updates: async ({ instanceId }: { instanceId: string }) => {
    const n = INSTANCES.find((i) => i.id === instanceId)?.mods.length ?? 0;
    await loadIcons(CATALOG.mod.slice(0, n).map(([, , , slug]) => slug ?? ""));
    return installed(n).filter((m) => m.update_available);
  },
  check_for_update: () => null,

  add_offline_account: ({ username }: { username: string }): Account => {
    const acc: Account = {
      id: `off-${Date.now()}`, username, account_type: "offline",
      /* Im echten Backend: UUID v3 ueber MD5("OfflinePlayer:" + name). */
      uuid: "00000000-0000-3000-8000-000000000000",
      active: false, expires_at: 0, offline: true, skin_url: null,
    };
    ACCOUNTS.push(acc);
    return acc;
  },

  discover_lan: (): LanServer[] => [
    { address: "192.168.1.42", port: 25565, motd: "Jonas Welt",
      discovered_at: new Date().toISOString() },
  ],

  list_worlds: () => [
    { folder: "Neue_Welt",  name: "Neue Welt",  last_played: "2026-09-27T18:00:00Z" },
    { folder: "Hardcore_3", name: "Hardcore 3", last_played: "2026-09-21T20:14:00Z" },
  ],
  detect_foreign_launchers: () => [
    { launcher: "Prism Launcher", path: "~/.local/share/PrismLauncher", count: 4 },
  ],
  suggest_jvm_flags: () => [
    "-XX:+UseZGC", "-XX:+ZGenerational", "-Xmx6G", "-Xms2G",
    "-XX:+AlwaysPreTouch", "-XX:+UseTransparentHugePages",
  ],
};

export async function mockInvoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  await sleep(60 + Math.random() * 180);          // spuerbare, aber kurze Latenz
  const h = handlers[cmd];
  if (!h) {
    console.warn(`[mock] unbehandelt: ${cmd}`, args);
    return undefined as T;
  }
  return h(args ?? {}) as T;
}

/** Im Mock feuert nur die Telemetrie, damit die Live-Anzeige etwas zeigt. */
export function mockListen<T>(event: string, handler: (p: T) => void): () => void {
  if (event !== "telemetry") return () => {};
  const timer = setInterval(() => {
    handler(handlers.get_telemetry!({ instanceId: "0c205d46" }) as T);
  }, 1200);
  return () => clearInterval(timer);
}
