/* =========================================================================
   Die vollstaendige Backend-Schnittstelle, nach Domaenen geordnet.

   Die mit (0.1.3) markierten Befehle sind aus dem alten Binary
   wiederhergestellt und in Name und Argumentform unveraendert uebernommen
   (recovered/spec/api-contract.md). Alles ohne Marke ist neu in 0.2.
   Die AI-Befehle aus 0.1.3 sind ersatzlos entfallen.
   ========================================================================= */

import { call, on } from "./bridge";
import type {
  Account, AiLogEvent, AiModel, AiStatus, AssistantSession, BedrockInstance, BedrockStatus,
  BedrockVersion, ConflictExplanation, Config,
  CrashDiagnosis, DeviceLoginPoll,
  CrashReport, Instance, InstalledMod, JavaInfo, SearchIntent,
  JoinTarget, LanServer, Loader, LoaderVersion, ModVersion, Progress, ProjectType,
  ResolvePlan, RunningBedrockInstance, RunningInstance, SavedServer, SearchResult, ServerStatus,
  Snapshot, Telemetry, VersionEntry, FabricMod, RunningRustInstance, RustBuildEvent, RustInstance, RustStatus, RustVariant,
} from "./types";

/* ---- Sonstiges --------------------------------------------------------- */
export const system = {
  /** Oeffnet eine http(s)-Adresse im Standardbrowser. */
  openExternal: (url: string) => call<void>("open_external", { url }),
};

/* ---- Konten ---------------------------------------------------------- */
export const auth = {
  list:        () => call<Account[]>("get_accounts"),                              // 0.1.3
  startDevice: () => call<{ user_code: string; verification_uri: string; device_code: string; interval: number }>(
                       "start_device_code_login"),                                  // 0.1.3
  startWebview:() => call<Account>("start_microsoft_login_webview"),                // 0.1.3
  poll:    (deviceCode: string) => call<DeviceLoginPoll>("poll_device_code_login", { deviceCode }), // 0.1.3
  activate:(accountId: string)  => call<void>("set_active_account", { accountId }), // 0.1.3
  remove:  (accountId: string)  => call<void>("remove_account", { accountId }),     // 0.1.3
  refresh: (accountId: string)  => call<Account>("refresh_account", { accountId }),

  /** Legt ein Konto ohne Microsoft-Anmeldung an. Die UUID wird wie in
      Minecrafts Offline-Modus aus "OfflinePlayer:<name>" gebildet, damit
      Weltdaten und Inventare zwischen Starts stabil bleiben. */
  addOffline: (username: string) => call<Account>("add_offline_account", { username }),
  renameOffline: (accountId: string, username: string) =>
                   call<Account>("rename_offline_account", { accountId, username }),
};

/* ---- Instanzen ------------------------------------------------------- */
export const instances = {
  list:   () => call<Instance[]>("list_instances"),                                 // 0.1.3
  create: (name: string, mcVersion: string, loader: Loader, useCustomUi = false) =>
            call<Instance>("create_instance", { name, mcVersion, loader, useCustomUi }), // 0.1.3
  update: (config: Instance) => call<Instance>("update_instance", { config }),      // 0.1.3
  trash:  (id: string) => call<void>("trash_instance", { id }),                     // 0.1.3
  restore:(id: string) => call<void>("restore_instance", { id }),                   // 0.1.3
  purge:  (id: string) => call<void>("permanently_delete_instance", { id }),        // 0.1.3
  listTrashed: () => call<Instance[]>("list_trashed_instances"),                    // 0.1.3

  duplicate: (id: string, name: string) => call<Instance>("duplicate_instance", { id, name }),
  setGroup:  (id: string, group: string | null) => call<void>("set_instance_group", { id, group }),
  openFolder:(id: string) => call<void>("open_instance_folder", { id }),
  /** Import aus Prism, MultiMC, CurseForge oder der Modrinth-App. */
  importForeign: (path: string) => call<Instance>("import_foreign_instance", { path }),
  detectForeign: () => call<{ launcher: string; path: string; count: number }[]>("detect_foreign_launchers"),
};

/* ---- Sicherungen ----------------------------------------------------- */
export const snapshots = {
  list:    (instanceId: string) => call<Snapshot[]>("list_snapshots", { instanceId }),
  create:  (instanceId: string, label: string, includeWorlds: boolean) =>
             call<Snapshot>("create_snapshot", { instanceId, label, includeWorlds }),
  restore: (snapshotId: string) => call<void>("restore_snapshot", { snapshotId }),
  remove:  (snapshotId: string) => call<void>("delete_snapshot", { snapshotId }),
};

/* ---- Versionen, Loader, Java ----------------------------------------- */
export const manifest = {
  /** Das vollstaendige Manifest, alle Kanaele. Gefiltert wird in der
      Oberflaeche, nicht hier -- so bleibt der Aufruf zwischenspeicherbar. */
  versions:       () => call<VersionEntry[]>("get_versions"),                       // 0.1.3
  download:       (mcVersion: string) => call<void>("download_version", { mcVersion }), // 0.1.3
  fabricVersions: (mcVersion: string) => call<string[]>("get_fabric_versions", { mcVersion }), // 0.1.3
  loaderVersions: (loader: Loader, mcVersion: string) =>
                    call<LoaderVersion[]>("get_loader_versions", { loader, mcVersion }),
  installLoader:  (instanceId: string, loader: Loader, version: string) =>
                    call<void>("install_loader", { instanceId, loader, version }),
};

export const java = {
  detect:   (mcVersion: string) => call<JavaInfo[]>("detect_java", { mcVersion }),   // 0.1.3
  check:    (javaPath: string)  => call<JavaInfo>("check_java_path", { javaPath }),  // 0.1.3
  systemRam:() => call<number>("get_system_memory"),                                 // 0.1.3
  /** Laedt ein passendes Temurin-JDK herunter und registriert es. */
  install:  (major: number) => call<JavaInfo>("install_java", { major }),
  listInstalled: () => call<JavaInfo[]>("list_installed_java"),
};

/* ---- Starten --------------------------------------------------------- */
export const launch = {
  /** Startet die Instanz. Mit `join` springt das Spiel direkt in einen
      Server, eine Welt oder ein Realm (Quick Play ab 1.20, davor
      --server/--port). */
  start:   (instanceId: string, join: JoinTarget = { kind: "none" }) =>
             call<void>("launch_instance", { instanceId, join }),                    // 0.1.3 (erweitert)
  stop:    (instanceId: string) => call<void>("stop_instance", { instanceId }),      // 0.1.3
  running: () => call<RunningInstance[]>("get_running_instances"),                   // 0.1.3
  log:     (instanceId: string) => call<string[]>("get_launch_log", { instanceId }), // 0.1.3
  clearLog:(instanceId: string) => call<void>("clear_launch_log", { instanceId }),   // 0.1.3
  telemetry:(instanceId: string) => call<Telemetry>("get_telemetry", { instanceId }),
  crashes: (instanceId: string) => call<CrashReport[]>("list_crash_reports", { instanceId }),
  /** Wertet das zuletzt geschriebene Protokoll aus und benennt die Ursache. */
  analyze: (instanceId: string) =>
             call<CrashDiagnosis | null>("analyze_log", { instanceId }),
  /** Wertet eine beliebige Protokolldatei aus, z. B. eine hineingezogene. */
  analyzeText: (text: string) => call<CrashDiagnosis | null>("analyze_log_text", { text }),
  /** Schlaegt JVM-Flags fuer RAM, Version und Modanzahl vor. */
  suggestFlags: (instanceId: string) => call<string[]>("suggest_jvm_flags", { instanceId }),
  /** Listet die Welten einer Instanz, damit Quick Play sie anspringen kann. */
  worlds: (instanceId: string) =>
            call<{ folder: string; name: string; last_played: string }[]>(
              "list_worlds", { instanceId }),
};

/* ---- LAN ------------------------------------------------------------- */
export const lan = {
  /** Hoert kurz auf Minecrafts LAN-Broadcast (224.0.2.60:4445). */
  discover: (timeoutMs = 3000) => call<LanServer[]>("discover_lan", { timeoutMs }),
};

/* ---- Mods ------------------------------------------------------------ */
export const mods = {
  search: (query: string, gameVersion: string | null, loader: Loader | null,
           projectType: ProjectType, page: number) =>
            call<{ results: SearchResult[]; total: number }>(
              "search_mods", { query, gameVersion, loader, projectType, page }),      // 0.1.3
  versions: (projectId: string, gameVersion: string | null, loader: Loader | null,
             provider?: string) =>
              call<ModVersion[]>("get_mod_versions", { projectId, gameVersion, loader, provider }), // 0.1.3
  install:  (instanceId: string, versionId: string, provider?: string) =>
              call<InstalledMod>("install_mod", { instanceId, versionId, provider }),  // 0.1.3
  installShader: (instanceId: string, versionId: string) =>
              call<string>("install_shader_pack", { instanceId, versionId }),
  installed:(instanceId: string) => call<InstalledMod[]>("list_instance_mods", { instanceId }), // 0.1.3
  remove:   (instanceId: string, modFilename: string) =>
              call<void>("remove_mod", { instanceId, modFilename }),                   // 0.1.3
  toggle:   (instanceId: string, modFilename: string) =>
              call<void>("toggle_mod", { instanceId, modFilename }),                   // 0.1.3
  addLocal: (instanceId: string, sourcePath: string) =>
              call<void>("add_local_mod", { instanceId, sourcePath }),                 // 0.1.3

  /** Plant eine Installation samt Abhaengigkeiten, bevor etwas geschrieben wird. */
  resolve:  (instanceId: string, versionId: string) =>
              call<ResolvePlan>("resolve_mod_install", { instanceId, versionId }),
  applyPlan:(instanceId: string, versionIds: string[]) =>
              call<void>("apply_mod_plan", { instanceId, versionIds }),
  checkUpdates: (instanceId: string) => call<InstalledMod[]>("check_mod_updates", { instanceId }),
  updateAll:    (instanceId: string) => call<void>("update_all_mods", { instanceId }),
  projectInfo:  (provider: string, projectId: string) =>
                  call<SearchResult>("get_project_info", { provider, projectId }),
};

/* ---- Modpacks -------------------------------------------------------- */
export const modpacks = {
  preview: (path: string) => call<{ name: string; version: string; mod_count: number }>(
                               "preview_modpack", { path }),                           // 0.1.3
  import:  (path: string) => call<Instance>("import_modpack", { path }),               // 0.1.3
  export:  (instanceId: string, options: Record<string, unknown>, outputPath: string) =>
             call<void>("export_modpack", { instanceId, options, outputPath }),         // 0.1.3
};

/* ---- Server ---------------------------------------------------------- */
export const servers = {
  list:   () => call<SavedServer[]>("list_servers"),                                   // 0.1.3
  save:   (name: string, address: string) => call<SavedServer>("save_server", { name, address }), // 0.1.3
  remove: (id: string) => call<void>("delete_server", { id }),                          // 0.1.3
  ping:   (address: string) => call<ServerStatus>("ping_server", { address }),          // 0.1.3
};

/* ---- Einstellungen und Aktualisierung -------------------------------- */
export const settings = {
  get:  () => call<Config>("get_config"),                                              // 0.1.3
  save: (config: Config) => call<void>("save_config", { config }),                      // 0.1.3
};

export const updater = {
  check:   () => call<{ version: string; url: string; notes: string } | null>("check_for_update"), // 0.1.3
  /** Laedt das Paket herunter und gibt den Pfad der fertigen Datei zurueck --
      eingespielt wird bewusst nicht automatisch, das bleibt dem Nutzer. */
  install: (url: string, filename?: string) =>
    call<string>("download_and_install_update", { url, filename }),                     // 0.1.3
};

/* ---- Assistent -------------------------------------------------------- */
/* Optional. Ohne hinterlegten Schlüssel meldet jede Funktion null, und die
   Oberfläche bleibt bei Regeltafel und Stichwortsuche. */
export const ai = {
  status: () => call<AiStatus>("ai_status"),
  setKey: (key: string | null) => call<AiStatus>("ai_set_key", { key }),
  models: () => call<AiModel[]>("ai_list_models"),
  setModel: (model: string) => call<void>("ai_set_model", { model }),
  /** Übersetzt einen Wunsch in Suchanfragen. */
  interpret: (wish: string) => call<SearchIntent | null>("ai_interpret_search", { wish }),
  explainConflict: (facts: string) =>
                     call<ConflictExplanation | null>("ai_explain_conflict", { facts }),
};

/* ---- CurseForge --------------------------------------------------------- */
export interface CurseforgeStatus { configured: boolean; key_hint: string | null; from_env: boolean }
export const curseforge = {
  status: () => call<CurseforgeStatus>("curseforge_status"),
  setKey: (key: string | null) => call<CurseforgeStatus>("curseforge_set_key", { key }),
};

/* ---- Sicherheit ---------------------------------------------------------
   Pflichtpruefung vor jedem Start (siehe launch::start im Backend). Warnt
   standardmaessig, statt zu blockieren -- ein einzelner Fehlalarm soll nie
   versehentlich das Spiel aussperren.                                    */
export type SecurityMode = "warn" | "block";

export interface SecurityStatus {
  virustotal_configured: boolean;
  virustotal_key_hint: string | null;
  clamav_available: boolean;
  mode: SecurityMode;
  auto_scan: boolean;
}

export interface FileVerdict {
  sha1: string;
  filename: string;
  flagged: boolean;
  reason: string;
  checked_at: string;
  allowed: boolean;
}

export const security = {
  status: () => call<SecurityStatus>("security_status"),
  setKey: (key: string | null) => call<SecurityStatus>("security_set_key", { key }),
  setMode: (mode: SecurityMode) => call<void>("security_set_mode", { mode }),
  setAutoScan: (enabled: boolean) => call<void>("security_set_auto_scan", { enabled }),
  scanInstance: (instanceId: string) => call<FileVerdict[]>("security_scan_instance", { instanceId }),
  history: () => call<FileVerdict[]>("security_history"),
  allowHash: (sha1: string) => call<void>("security_allow_hash", { sha1 }),
  uploadUnknown: (instanceId: string, filename: string) =>
                   call<void>("security_upload_unknown", { instanceId, filename }),
};

/* ---- Bedrock ------------------------------------------------------------ */
export const bedrock = {
  status: () => call<BedrockStatus>("bedrock_status"),
  openUi: () => call<void>("bedrock_open_ui"),
  listVersions: () => call<BedrockVersion[]>("bedrock_list_versions"),
  listInstances: () => call<BedrockInstance[]>("bedrock_list_instances"),
  createInstance: (name: string, versionDirName: string) =>
                    call<BedrockInstance>("bedrock_create_instance", { name, versionDirName }),
  start: (instanceId: string) => call<RunningBedrockInstance>("bedrock_start", { instanceId }),
  stop: (instanceId: string) => call<void>("bedrock_stop", { instanceId }),
  running: () => call<RunningBedrockInstance[]>("bedrock_running"),
};

/* ---- Instanz-Assistent ------------------------------------------------ */
/* Höchstens zwei Modellanfragen: Fragebogen zuschneiden, Plan begründen.
   Gesucht wird dazwischen ohne Modell. Ohne Schlüssel läuft alles über den
   festen Katalog — dann fehlen nur die Begründungen. */
export const assistant = {
  start: (goal: string, model?: string, research?: boolean) =>
           call<AssistantSession>("assistant_start", { goal, model, research }),
  load: (sessionId: string) => call<AssistantSession>("assistant_load", { sessionId }),
  openSessions: () => call<AssistantSession[]>("assistant_open_sessions"),
  discard: (sessionId: string) => call<void>("assistant_discard", { sessionId }),
  answer: (sessionId: string, questionId: string, choices: string[]) =>
            call<AssistantSession>("assistant_answer", { sessionId, questionId, choices }),
  candidates: (sessionId: string) =>
                call<AssistantSession>("assistant_candidates", { sessionId }),
  select: (sessionId: string, projectIds: string[]) =>
            call<AssistantSession>("assistant_select", { sessionId, projectIds }),
  plan: (sessionId: string, name?: string, model?: string) =>
          call<AssistantSession>("assistant_plan", { sessionId, name, model }),
  apply: (sessionId: string) => call<Instance>("assistant_apply", { sessionId }),
};

/* ---- Rust-Edition (Horizon) ------------------------------------------- */
export const rust = {
  status: (variant: RustVariant) => call<RustStatus>("rust_status", { variant }),
  build: (variant: RustVariant) => call<void>("rust_build", { variant }),
  prepare: (variant: RustVariant) => call<void>("rust_prepare", { variant }),
  listInstances: (variant: RustVariant) => call<RustInstance[]>("rust_list_instances", { variant }),
  createInstance: (variant: RustVariant, name: string, quickServer?: string) =>
    call<RustInstance>("rust_create_instance", { variant, name, quickServer }),
  updateInstance: (variant: RustVariant, instanceId: string,
                   patch: { name?: string; quickServer?: string; quickWorld?: string; fullscreen?: boolean; jvmHeapMb?: number }) =>
    call<RustInstance>("rust_update_instance", { variant, instanceId, ...patch }),
  deleteInstance: (variant: RustVariant, instanceId: string) => call<void>("rust_delete_instance", { variant, instanceId }),
  start: (variant: RustVariant, instanceId: string) => call<RunningRustInstance>("rust_start", { variant, instanceId }),
  stop: (instanceId: string) => call<void>("rust_stop", { instanceId }),
  running: () => call<RunningRustInstance[]>("rust_running"),
};

/* ---- BHH: Fabric-Mods ---------------------------------------------------- */
export const bhh = {
  listMods: (instanceId: string) => call<FabricMod[]>("bhh_list_mods", { instanceId }),
  addMod: (instanceId: string, path: string) => call<FabricMod>("bhh_add_mod", { instanceId, path }),
  pickMods: (instanceId: string) => call<FabricMod[]>("bhh_pick_mods", { instanceId }),
  removeMod: (instanceId: string, file: string) => call<void>("bhh_remove_mod", { instanceId, file }),
  openModsDir: (instanceId: string) => call<void>("bhh_open_mods_dir", { instanceId }),
};

/* ---- Ereignisse ------------------------------------------------------ */
export const events = {
  downloadProgress: (h: (p: Progress) => void) => on<Progress>("download-progress", h),         // 0.1.3
  modpackProgress:  (h: (p: Progress) => void) => on<Progress>("modpack-download-progress", h), // 0.1.3
  updateProgress:   (h: (p: Progress) => void) => on<Progress>("update-download-progress", h),  // 0.1.3
  instanceStopped:  (h: (p: { instance_id: string; exit_code: number }) => void) =>
                      on("instance-stopped", h),                                                // 0.1.3
  bedrockInstanceStopped: (h: (p: { instance_id: string; exit_code: number }) => void) =>
                      on("bedrock-instance-stopped", h),
  rustInstanceStopped: (h: (p: { instance_id: string; exit_code: number; played_seconds: number; crash_report: string | null }) => void) =>
                      on("rust-instance-stopped", h),
  rustBuild:   (h: (p: RustBuildEvent) => void) => on<RustBuildEvent>("rust-build", h),
  rustPrepareProgress: (h: (p: Progress) => void) => on<Progress>("rust-prepare-progress", h),
  rustLogLine: (h: (p: { instance_id: string; line: string }) => void) => on("rust-log-line", h),
  updateAvailable:  (h: (p: { version: string }) => void) => on("update-available", h),         // 0.1.3
  logLine:     (h: (p: { instance_id: string; line: string }) => void) => on("log-line", h),
  telemetry:   (h: (p: Telemetry) => void) => on<Telemetry>("telemetry", h),
  crashed:     (h: (p: CrashReport) => void) => on<CrashReport>("instance-crashed", h),
  aiLog:       (h: (p: AiLogEvent) => void) => on<AiLogEvent>("assistant-ai-log", h),
};
