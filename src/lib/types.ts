/* =========================================================================
   Datenmodell
   Feldnamen folgen exakt dem aus Version 0.1.3 wiederhergestellten Schema
   (siehe recovered/spec/data-model.md). Das ist kein Zufall: eine bestehende
   Installation unter ~/.local/share/buildmc-launcher/ muss ohne Migration
   weiterlaufen. Neue Felder sind additiv und optional.
   ========================================================================= */

export type Loader =
  | "vanilla"
  | "buildmc"   /* eigener Client-Mod, aus Bestandsdaten 0.1.3 */
  | "fabric"
  | "forge"
  | "neoforge"
  | "quilt";
export type ProjectType =
  | "mod"
  | "modpack"
  | "resourcepack"
  | "shader"
  | "datapack";
export type Provider = "modrinth" | "curseforge";

/* ---- Instanz --------------------------------------------------------- */

export interface Instance {
  id: string;
  name: string;
  mc_version: string;
  loader: Loader;
  fabric_version: string | null;
  buildmc_mod_version: string | null;
  mods: string[];
  resource_packs: string[];
  shader_packs: string[];
  jvm_args: string[];
  min_ram: number;
  max_ram: number;
  use_custom_ui: boolean;
  created_at: string;
  last_played: string;

  /* --- neu in 0.2 --- */
  group?: string | null;
  icon?: string | null;
  accent?: string | null;
  notes?: string;
  locked?: boolean;
  play_seconds?: number;
  launch_count?: number;
  java_path_override?: string | null;
}

export interface InstanceGroup {
  name: string;
  collapsed: boolean;
}

/* ---- Mods ------------------------------------------------------------ */

export interface InstalledMod {
  filename: string;
  name: string;
  version: string;
  enabled: boolean;
  provider: Provider | null;
  project_id: string | null;
  version_id: string | null;
  icon_url: string | null;
  /** Vom Update-Scan gesetzt: neuere passende Version am Server. */
  update_available: string | null;
  /** Dateinamen anderer Mods, die diese Mod braucht. */
  depends_on: string[];
  size_bytes: number;
}

export interface SearchResult {
  provider: Provider;
  project_id: string;
  slug: string;
  title: string;
  description: string;
  author: string;
  downloads: number;
  follows: number;
  icon_url: string | null;
  categories: string[];
  project_type: ProjectType;
  loaders: Loader[];
  game_versions: string[];
  updated_at: string;
  license: string | null;
  client_side: "required" | "optional" | "unsupported";
  server_side: "required" | "optional" | "unsupported";
}

export interface ModVersion {
  version_id: string;
  provider: Provider;
  name: string;
  version_number: string;
  changelog: string | null;
  release_type: "release" | "beta" | "alpha";
  downloads: number;
  published_at: string;
  game_versions: string[];
  loaders: Loader[];
  filename: string;
  size_bytes: number;
  sha1: string | null;
  dependencies: ModDependency[];
}

export interface ModDependency {
  project_id: string | null;
  version_id: string | null;
  kind: "required" | "optional" | "incompatible" | "embedded";
  name: string | null;
}

/** Ergebnis der Vorabpruefung, bevor tatsaechlich installiert wird. */
export interface ResolvePlan {
  install: ModVersion[];
  already_present: string[];
  conflicts: ResolveConflict[];
  total_bytes: number;
}

export interface ResolveConflict {
  kind: "incompatible" | "duplicate" | "wrong_loader" | "wrong_game_version";
  subject: string;
  detail: string;
}

/* ---- Konten, Server -------------------------------------------------- */

export type AccountType = "microsoft" | "offline";

export interface Account {
  id: string;
  username: string;
  uuid: string;
  account_type: AccountType;
  active: boolean;
  /** Nur bei Microsoft-Konten gesetzt. */
  expires_at: number;
  /** Bei Offline-Konten true: keine Sitzung, keine Servervalidierung. */
  offline?: boolean;
  skin_url?: string | null;
}

/** Ergebnis einer einzelnen Abfrage waehrend der Geraetecode-Anmeldung.
    `slow_down` muss von `pending` unterschieden werden -- sonst fragt die
    Oberflaeche im selben Abstand weiter, statt den Abstand zu vergroessern. */
export type DeviceLoginPoll =
  | { status: "pending" }
  | { status: "slow_down" }
  | { status: "ready"; account: Account };

/* ---- Beitrittswege ---------------------------------------------------- */

/** Wohin das Spiel direkt nach dem Start springen soll.
    Setzt die Quick-Play-Argumente (ab 1.20) bzw. --server/--port davor. */
export type JoinTarget =
  | { kind: "none" }
  | { kind: "server"; address: string }
  | { kind: "world"; folder: string }
  | { kind: "realm"; realm_id: string };

export interface LanServer {
  address: string;
  port: number;
  motd: string;
  discovered_at: string;
}

export interface SavedServer {
  id: string;
  name: string;
  address: string;
  favorite: boolean;
  status?: ServerStatus | null;
}

export interface ServerStatus {
  online: boolean;
  players_online: number;
  players_max: number;
  version: string;
  motd: string;
  latency_ms: number;
  favicon: string | null;
}

/* ---- Java, Laufzeit -------------------------------------------------- */

export interface JavaInfo {
  path: string;
  version: string;
  major: number;
  vendor: string;
  arch: string;
  is_bundled: boolean;
}

export interface RunningInstance {
  instance_id: string;
  pid: number;
  started_at: string;
}

/** Live-Werte eines laufenden Spiels.
 *
 *  Gemessen wird von außen über den Prozess. Java-Halde und
 *  Garbage-Collector-Pausen stehen bewusst nicht darin — die ließen sich
 *  nur mit einer Anbindung im Spiel selbst lesen, und erfundene Zahlen
 *  wären schlimmer als keine. */
export interface Telemetry {
  instance_id: string;
  /** Tatsächlich belegter Arbeitsspeicher des Prozesses. */
  rss_bytes: number;
  /** Zugestandene Obergrenze, aus dem gesetzten -Xmx. */
  max_bytes: number;
  cpu_percent: number;
  uptime_seconds: number;
}

export interface CrashReport {
  instance_id: string;
  occurred_at: string;
  exit_code: number;
  signature: string;
  /** Erkannte Ursache samt Vorschlag, oder null wenn unbekannt. */
  diagnosis: CrashDiagnosis | null;
  log_excerpt: string;
}

export interface CrashDiagnosis {
  cause: string;
  explanation: string;
  suggestions: string[];
  culprit_mod: string | null;
}

/* ---- Fortschritt ----------------------------------------------------- */

export interface Progress {
  id: string;
  label: string;
  detail: string;
  current: number;
  total: number;
  bytes_per_second: number;
  done: boolean;
  error: string | null;
}

/* ---- Sicherungen ----------------------------------------------------- */

export interface Snapshot {
  id: string;
  instance_id: string;
  label: string;
  created_at: string;
  size_bytes: number;
  includes_worlds: boolean;
  automatic: boolean;
}

/* ---- Konfiguration --------------------------------------------------- */

export interface Config {
  theme: "dark" | "light";
  accent_color: string;
  background_shade: number;
  min_ram: number;
  max_ram: number;
  java_path: string;
  use_zgc: boolean;
  concurrent_downloads: number;
  verify_sha1: boolean;
  close_on_launch: boolean;
  auto_update: boolean;
  skipped_update_version: string | null;
  discord_rpc: boolean;
  performance_mode: boolean;
  high_process_priority: boolean;
  easter_eggs_enabled: boolean;
  easter_egg_frequency: number;
  ai_reasoning?: boolean;
  ai_tools?: boolean;

  /* --- neu in 0.2 --- */
  language?: string;
  auto_snapshot?: boolean;
  snapshot_keep?: number;
  mod_update_check?: boolean;
  curseforge_enabled?: boolean;
  telemetry_overlay?: boolean;
  assistant_web_research?: boolean;
  default_edition?: "java" | "bedrock" | "rust" | "bhh";
  /** Eigene Microsoft-Anmeldung; aus bis zur Freigabe der App-ID durch Mojang. */
  own_login?: boolean;
}

/* ---- Rust-Edition (Horizon) -------------------------------------------- */

export type RustVariant = "horizon" | "bhh";

export interface RustStatus {
  variant: RustVariant;
  jdk_found: boolean;
  bundled: boolean;
  source_found: boolean;
  source_revision: string;
  cargo_found: boolean;
  built: boolean;
  outdated: boolean;
  horizon_version: string;
  launch_api: string;
  prepared: boolean;
  microsoft_account: boolean;
  account_name: string;
  minecraft_version: string;
  building: boolean;
  preparing: boolean;
}

export interface RustInstance {
  id: string;
  name: string;
  created_at: string;
  last_played: string;
  play_seconds: number;
  launch_count: number;
  quick_server: string;
  quick_world: string;
  fullscreen: boolean;
  variant: RustVariant;
  jvm_heap_mb: number;
}

export interface FabricMod {
  file: string;
  id: string;
  name: string;
  version: string;
  status: "bereit" | "unvollstaendig" | "kein_fabric_mod";
  reasons: string[];
  missing_classes: string[];
  used_classes: number;
}

export interface RunningRustInstance {
  instance_id: string;
  pid: number;
  started_at: string;
}

export interface RustBuildEvent {
  variant: RustVariant;
  line: string;
  fraction: number;
  done: boolean;
  error?: string | null;
}

/* ---- Bedrock ----------------------------------------------------------- */

export interface BedrockStatus {
  platform_supported: boolean;
  client_found: boolean;
  client_path: string | null;
  ui_found: boolean;
  ui_path: string | null;
  google_account_linked: boolean;
  available_versions: number;
}

export interface BedrockVersion {
  dir_name: string;
}

export interface BedrockInstance {
  id: string;
  name: string;
  version_dir_name: string;
  created_at: string;
  last_played: string;
  play_seconds: number;
  launch_count: number;
}

export interface RunningBedrockInstance {
  instance_id: string;
  pid: number;
  started_at: string;
}

/* Mojang liefert im Manifest nur vier Typen. Pre-Releases und Release
   Candidates stecken alle im Typ "snapshot" und sind nur am Namen zu
   erkennen. Wir loesen das beim Einlesen in einen echten Kanal auf,
   damit die Oberflaeche danach filtern kann. */
export type VersionChannel =
  | "release"
  | "release_candidate"
  | "pre_release"
  | "snapshot"
  | "experimental"   /* April-Scherze und Sonderbauten, z.B. 23w13a_or_b */
  | "old_beta"
  | "old_alpha";

export type ManifestType = "release" | "snapshot" | "old_beta" | "old_alpha";

export interface LoaderVersion {
  version: string;
  stable: boolean;
  installed: boolean;
}

export interface VersionEntry {
  id: string;
  /** Rohtyp aus dem Mojang-Manifest. */
  kind: ManifestType;
  /** Abgeleiteter Kanal -- danach filtert die Oberflaeche. */
  channel: VersionChannel;
  released_at: string;
  installed: boolean;
  /** Fuer diese Version noetige Java-Hauptversion, aus dem Manifest. */
  java_major?: number;
  /** Loader, die es fuer diese Version gibt. */
  available_loaders?: Loader[];
}

/* ---- Assistent (optional, per API-Schlüssel) -------------------------- */

export interface AiStatus {
  configured: boolean;
  /** Abgeblendeter Schlüssel, nie der volle Wert. */
  key_hint: string | null;
  model: string;
  /** Kommt der Schlüssel aus der Umgebung statt aus der Ablage? */
  from_env: boolean;
  /** Parameter, die dieses Modell abgelehnt hat und die deshalb entfallen. */
  rejected: string[];
}

export interface AiModel {
  id: string;
  description: string | null;
}

/** Ergebnis der Absichtserkennung. Das Modell liefert Suchbegriffe,
    keine Projekte -- gesucht wird weiterhin bei den Portalen. */
export interface SearchIntent {
  queries: string[];
  project_type: ProjectType | null;
  game_version: string | null;
  loader: Loader | null;
  /** Wie das Modell den Wunsch verstanden hat. */
  reading: string;
}

export interface ConflictExplanation {
  summary: string;
  detail: string;
  options: string[];
}

/* ---- Instanz-Assistent ------------------------------------------------ */

export type AssistantStep = "describe" | "answer" | "choose" | "review" | "done";

export interface AssistantOption {
  id: string;
  label: string;
  detail: string;
  queries: string[];
  project_type: ProjectType | null;
}

export interface AssistantQuestion {
  id: string;
  prompt: string;
  /** Kurzer Hinweis, warum das gefragt wird. */
  why: string;
  multi: boolean;
  options: AssistantOption[];
}

/** Ein Vorschlag. Die Projektfelder sind eingebettet. */
export interface AssistantCandidate extends SearchResult {
  /** Zu welchen Suchbegriffen dieser Treffer gehört. */
  matched: string[];
  preselected: boolean;
  /** false, wenn nur gefunden, weil der Versionsfilter fallen gelassen
      wurde -- passt möglicherweise nicht exakt. */
  version_exact: boolean;
}

export interface PlannedMod {
  project_id: string;
  title: string;
  icon_url: string | null;
  reason: string;
  /** Von der Optimierungsstufe beigesteuert, nicht gewählt. */
  from_optimization: boolean;
}

export interface AssistantPlan {
  name: string;
  mc_version: string;
  loader: Loader;
  loader_version: string | null;
  min_ram: number;
  max_ram: number;
  optimization: string | null;
  frame_cap: number | null;
  jvm_args: string[];
  mods: PlannedMod[];
  /** Begründung in eigenen Worten — nur wenn ein Modell beteiligt war. */
  rationale: string | null;
  notes: string[];
}

export interface AssistantSession {
  id: string;
  goal: string;
  step: AssistantStep;
  questions: AssistantQuestion[];
  answers: Record<string, string[]>;
  candidates: AssistantCandidate[];
  /** Ob die Suche gelaufen ist — leer heißt sonst „lädt noch". */
  searched: boolean;
  selected: string[];
  plan: AssistantPlan | null;
  /** `modell` oder `katalog` — womit der Fragebogen entstand. */
  source: string;
  created_at: string;
  notes: string[];
}

/** Ein Zwischenschritt eines laufenden Modellaufrufs -- live waehrend
    "katalog" (Fragebogen) oder "plan" (Begruendung). */
export type AiLogEvent =
  | { session_id: string; step: string; kind: "thinking"; text: string }
  | { session_id: string; step: string; kind: "writing" }
  | { session_id: string; step: string; kind: "tool_started"; name: string }
  | { session_id: string; step: string; kind: "tool_args"; delta: string }
  | { session_id: string; step: string; kind: "tool_done";
      query: string | null; results: { title: string; domain: string; url: string }[] }
  | { session_id: string; step: string; kind: "retry"; reason: string }
  | { session_id: string; step: string; kind: "done";
      prompt_tokens: number; completion_tokens: number };
