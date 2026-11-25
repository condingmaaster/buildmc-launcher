/* =========================================================================
   Anwendungszustand.

   Bewusst flach gehalten: ein Store fuer die Dinge, die mehrere Ansichten
   teilen (Instanzen, Konten, Konfiguration, laufende Spiele). Alles, was
   nur eine Ansicht betrifft, bleibt lokaler Komponentenzustand.
   ========================================================================= */

import { create } from "zustand";
import * as api from "./api";
import { hexToHsl } from "./format";
import type {
  Account, BedrockInstance, Config, Instance, JoinTarget, Progress, RunningBedrockInstance,
  RunningInstance, RunningRustInstance, RustInstance, RustVariant, Telemetry,
} from "./types";

export type Edition = "java" | "bedrock" | "rust" | "bhh";

export type View =
  | { name: "library" }
  | { name: "instance"; id: string }
  | { name: "browse" }
  | { name: "assistant" }
  | { name: "servers" }
  | { name: "settings" }
  | { name: "security" }
  | { name: "bedrock" }
  | { name: "rust" }
  | { name: "bhh" };

interface State {
  ready: boolean;
  view: View;
  instances: Instance[];
  accounts: Account[];
  running: RunningInstance[];
  telemetry: Telemetry | null;
  config: Config | null;
  toasts: Toast[];
  /** Laufender Download (Version, Mod, Modpack) -- `null` heisst: gerade
      nichts zu zeigen. Das Backend meldet echten Fortschritt (Prozent,
      Geschwindigkeit), vorher gab es dafuer nur einen Knopf-Spinner ohne
      jede Zahl -- bei einem grossen Download (Java, tausende Assets)
      wirkte das wie eingefroren, obwohl laengst etwas lief. */
  progress: Progress | null;

  /** Welche Spielart gerade aktiv ist -- orthogonal zu `view`, nicht Teil
      davon: Java und Bedrock haben eigene, unterschiedlich lange
      Navigationslisten, kein gemeinsames Icon-Set. */
  edition: Edition;
  bedrockInstances: BedrockInstance[];
  bedrockRunning: RunningBedrockInstance[];
  /** `mcpelauncher` deckt kein Windows ab -- dort bleibt der
      Editions-Schalter ausgeblendet, statt eine tote Bedrock-Seite zu
      zeigen. */
  bedrockPlatformSupported: boolean;
  /** Rust-Editionen (Horizon und BHH, von BuildMC selbst gebaut und gestartet). Namen fuer Meldungen. */
  rustInstances: RustInstance[];
  rustRunning: RunningRustInstance[];

  go: (v: View) => void;
  boot: () => Promise<void>;
  reloadInstances: () => Promise<void>;
  reloadAccounts: () => Promise<void>;
  reloadRunning: () => Promise<void>;
  startInstance: (id: string, join?: JoinTarget) => Promise<void>;
  stopInstance: (id: string) => Promise<void>;
  patchConfig: (patch: Partial<Config>) => Promise<void>;
  notify: (t: Omit<Toast, "id">) => void;
  dismiss: (id: number) => void;

  setEdition: (e: Edition) => void;
  reloadBedrockInstances: () => Promise<void>;
  reloadBedrockRunning: () => Promise<void>;
  startBedrockInstance: (id: string) => Promise<void>;
  stopBedrockInstance: (id: string) => Promise<void>;
  reloadRustInstances: () => Promise<void>;
  reloadRustRunning: () => Promise<void>;
  startRustInstance: (id: string, variant: RustVariant) => Promise<void>;
  stopRustInstance: (id: string) => Promise<void>;
}

export interface Toast {
  id: number;
  kind: "info" | "ok" | "warn" | "risk";
  title: string;
  detail?: string;
}

let toastSeq = 0;

export const useApp = create<State>((set, get) => ({
  ready: false,
  view: { name: "library" },
  instances: [],
  accounts: [],
  running: [],
  telemetry: null,
  config: null,
  toasts: [],
  progress: null,
  edition: "java",
  bedrockInstances: [],
  bedrockRunning: [],
  bedrockPlatformSupported: true,
  rustInstances: [],
  rustRunning: [],

  go: (view) => set({ view }),

  async boot() {
    const [instances, accounts, config, running, bedrockInstances, bedrockRunning, bedrockStatus, rustInstances, rustRunning] = await Promise.all([
      api.instances.list(), api.auth.list(), api.settings.get(), api.launch.running(),
      api.bedrock.listInstances().catch(() => []),
      api.bedrock.running().catch(() => []),
      api.bedrock.status().catch(() => null),
      Promise.all([api.rust.listInstances("horizon").catch(() => []), api.rust.listInstances("bhh").catch(() => [])])
        .then(([a, b]) => [...a, ...b]),
      api.rust.running().catch(() => []),
    ]);
    const edition: Edition = config.default_edition === "bedrock" ? "bedrock"
      : config.default_edition === "rust" ? "rust" : config.default_edition === "bhh" ? "bhh" : "java";
    applyTheme(config);
    applyEditionAccent(config, edition);
    set({
      instances, accounts, config, running, bedrockInstances, bedrockRunning, rustInstances, rustRunning, edition, ready: true,
      view: editionHome(edition),
      bedrockPlatformSupported: bedrockStatus?.platform_supported ?? true,
    });

    api.events.telemetry((telemetry) => set({ telemetry }));
    api.events.rustInstanceStopped(({ instance_id, exit_code, crash_report }) => {
      set((s) => ({ rustRunning: s.rustRunning.filter((r) => r.instance_id !== instance_id) }));
      void get().reloadRustInstances();
      const name = get().rustInstances.find((i) => i.id === instance_id)?.name ?? "Spiel";
      if (exit_code === 0) get().notify({ kind: "info", title: `${name} beendet` });
      else get().notify({ kind: "risk", title: `${name} ist abgestürzt (Code ${exit_code})`,
                          detail: crash_report ? `Absturzbericht: ${crash_report}` : undefined });
    });
    api.events.bedrockInstanceStopped(({ instance_id }) => {
      set((s) => ({ bedrockRunning: s.bedrockRunning.filter((r) => r.instance_id !== instance_id) }));
      const name = get().bedrockInstances.find((i) => i.id === instance_id)?.name ?? "Spiel";
      get().notify({ kind: "info", title: `${name} beendet` });
    });

    const onProgress = (p: Progress) => {
      set({ progress: p });
      if (p.done) {
        // Kurz stehen lassen, damit "fertig" sichtbar wird, statt
        // ersatzlos zu verschwinden -- dann erst ausblenden.
        setTimeout(() => {
          if (get().progress?.id === p.id) set({ progress: null });
        }, 1500);
      }
    };
    api.events.downloadProgress(onProgress);
    api.events.modpackProgress(onProgress);
    api.events.instanceStopped(({ instance_id }) => {
      set((s) => ({ running: s.running.filter((r) => r.instance_id !== instance_id) }));
      const name = get().instances.find((i) => i.id === instance_id)?.name ?? "Spiel";
      get().notify({ kind: "info", title: `${name} beendet` });
    });

    // Zweites Netz gegen das Ereignis oben: geht es einmal verloren --
    // etwa weil es genau in der schmalen Lücke zwischen dem ersten Laden
    // und dem Registrieren des Abos feuert --, bliebe eine Instanz sonst
    // für den Rest der Sitzung fälschlich als "läuft" stehen, mit einem
    // "Beenden"-Knopf, der nur noch "läuft nicht" zurückgibt.
    setInterval(() => { void get().reloadRunning(); }, 10_000);
    setInterval(() => { void get().reloadBedrockRunning(); }, 10_000);
    setInterval(() => { void get().reloadRustRunning(); }, 10_000);
  },

  async reloadInstances() {
    set({ instances: await api.instances.list() });
  },

  async reloadAccounts() {
    set({ accounts: await api.auth.list() });
  },

  async reloadRunning() {
    set({ running: await api.launch.running() });
  },

  /* Start und Stopp liegen im Store, weil beide Ansichten denselben
     Laufzustand zeigen -- sonst haengt eine davon hinterher. */
  async startInstance(id, join = { kind: "none" }) {
    /* Ohne eigene Anmeldung läuft der Start über den offiziellen Minecraft-Launcher: Profil anlegen,
       Launcher öffnen, Anmeldung und Spielstart übernimmt er. Ein Beitrittsziel lässt sich so nicht mitgeben.
       Mit eigener Anmeldung (erst nach Mojangs Freigabe) und einem Microsoft-Konto startet BuildMC selbst. */
    const name = get().instances.find((i) => i.id === id)?.name ?? "Instanz";
    const selbst = get().config?.own_login && get().accounts.some((a) => a.active && a.account_type !== "offline");
    if (selbst) {
      try {
        await api.launch.start(id, join);
        await get().reloadRunning();
        get().notify({
          kind: "ok",
          title: `${name} wird gestartet`,
          detail: join.kind === "server" ? `Verbindet zu ${join.address}` : undefined,
        });
      } catch (e) {
        get().notify({ kind: "risk", title: "Start fehlgeschlagen", detail: String(e) });
        throw e;
      }
      return;
    }
    try {
      const detail = await api.launch.official(id);
      get().notify({
        kind: "ok",
        title: `${name}: Minecraft-Launcher`,
        detail: join.kind === "server" ? `${detail} Server: ${join.address}` : detail,
      });
    } catch (e) {
      get().notify({ kind: "risk", title: "Übergabe fehlgeschlagen", detail: String(e) });
      throw e;
    }
  },

  async stopInstance(id) {
    const name = get().instances.find((i) => i.id === id)?.name ?? "Instanz";
    try {
      await api.launch.stop(id);
      await get().reloadRunning();
      get().notify({ kind: "info", title: `${name} beendet` });
    } catch (e) {
      // Sagt das Backend "läuft nicht", ist das die verlässlichste Auskunft,
      // die es geben kann -- die Anzeige muss dem folgen, auch wenn das
      // eigentliche Ereignis zum Beenden aus irgendeinem Grund nie ankam.
      // Sonst hängt der "Beenden"-Knopf für immer an einer toten Instanz.
      if (String(e).includes("läuft nicht")) {
        set((s) => ({ running: s.running.filter((r) => r.instance_id !== id) }));
      }
      get().notify({ kind: "risk", title: "Beenden fehlgeschlagen", detail: String(e) });
      throw e;
    }
  },

  async patchConfig(patch) {
    const current = get().config;
    if (!current) return;
    const next = { ...current, ...patch };
    applyTheme(next);
    // Getrennt von applyTheme, nicht als zusaetzlicher Parameter davon --
    // sonst wuerde jeder patchConfig-Aufruf (auch der von setEdition selbst)
    // die Bedrock-Akzentfarbe sofort wieder auf die aus `next` berechnete
    // Java-Farbe zuruecksetzen.
    applyEditionAccent(next, get().edition);
    set({ config: next });
    await api.settings.save(next);
  },

  notify(t) {
    const id = ++toastSeq;
    set((s) => ({ toasts: [...s.toasts, { ...t, id }] }));
    setTimeout(() => get().dismiss(id), 5200);
  },

  dismiss(id) {
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },

  setEdition(edition) {
    set({ edition, view: editionHome(edition) });
    const config = get().config;
    if (config) applyEditionAccent(config, edition);
    void get().patchConfig({ default_edition: edition });
  },

  async reloadRustInstances() {
    const [a, b] = await Promise.all([api.rust.listInstances("horizon").catch(() => []), api.rust.listInstances("bhh").catch(() => [])]);
    set({ rustInstances: [...a, ...b] });
  },

  async reloadRustRunning() {
    set({ rustRunning: await api.rust.running().catch(() => []) });
  },

  async startRustInstance(id, variant) {
    const name = get().rustInstances.find((i) => i.id === id)?.name ?? "Instanz";
    try {
      await api.rust.start(variant, id);
      await get().reloadRustRunning();
      get().notify({ kind: "ok", title: `${name} wird gestartet`, detail: variant === "bhh" ? "BuildMC Horizon Hybrid" : "Horizon" });
    } catch (e) {
      get().notify({ kind: "risk", title: "Start fehlgeschlagen", detail: String(e) });
      throw e;
    }
  },

  async stopRustInstance(id) {
    try {
      await api.rust.stop(id);
    } catch (e) {
      if (String(e).includes("läuft nicht")) {
        set((s) => ({ rustRunning: s.rustRunning.filter((r) => r.instance_id !== id) }));
      } else {
        get().notify({ kind: "risk", title: "Beenden fehlgeschlagen", detail: String(e) });
      }
    }
    await get().reloadRustRunning();
  },

  async reloadBedrockInstances() {
    set({ bedrockInstances: await api.bedrock.listInstances() });
  },

  async reloadBedrockRunning() {
    set({ bedrockRunning: await api.bedrock.running() });
  },

  async startBedrockInstance(id) {
    const name = get().bedrockInstances.find((i) => i.id === id)?.name ?? "Instanz";
    try {
      await api.bedrock.start(id);
      await get().reloadBedrockRunning();
      get().notify({ kind: "ok", title: `${name} wird gestartet` });
    } catch (e) {
      get().notify({ kind: "risk", title: "Start fehlgeschlagen", detail: String(e) });
      throw e;
    }
  },

  async stopBedrockInstance(id) {
    const name = get().bedrockInstances.find((i) => i.id === id)?.name ?? "Instanz";
    try {
      await api.bedrock.stop(id);
      await get().reloadBedrockRunning();
      get().notify({ kind: "info", title: `${name} beendet` });
    } catch (e) {
      if (String(e).includes("läuft nicht")) {
        set((s) => ({ bedrockRunning: s.bedrockRunning.filter((r) => r.instance_id !== id) }));
      }
      get().notify({ kind: "risk", title: "Beenden fehlgeschlagen", detail: String(e) });
      throw e;
    }
  },
}));

/** Schreibt Theme, Akzent und Bewegungsstufe an die Wurzel -- alles andere
    leitet sich in CSS daraus ab. */
export function applyTheme(c: Config | null) {
  if (!c) return;
  const root = document.documentElement;
  root.dataset.theme = c.theme;
  root.dataset.motion = c.performance_mode ? "calm" : "kinetic";
  const hsl = hexToHsl(c.accent_color);
  if (hsl) {
    root.style.setProperty("--accent-h", String(hsl.h));
    root.style.setProperty("--accent-s", `${Math.max(hsl.s, 55)}%`);
    root.style.setProperty("--accent-l", `${Math.min(Math.max(hsl.l, 52), 70)}%`);
  }
  root.style.setProperty("--shade", String(c.background_shade));
}

/** Faerbt die Akzentfarbe je Spielart um -- eigenstaendig, **nicht** als
    Parameter von `applyTheme`: `applyTheme` setzt `--accent-h/-s/-l` als
    Inline-Style, der schlaegt jede CSS-Regel mit Attribut-Selektor. Eine
    reine CSS-Ueberschreibung ueber `[data-edition="bedrock"]` haette darum
    nichts bewirkt -- die Umfaerbung muss hier, in JavaScript, passieren,
    nach jedem Aufruf von `applyTheme` erneut. */
/** Startseite einer Spielart. */
export function editionHome(e: Edition): View {
  return e === "bedrock" ? { name: "bedrock" } : e === "rust" ? { name: "rust" } : e === "bhh" ? { name: "bhh" } : { name: "library" };
}

export function applyEditionAccent(c: Config, edition: Edition) {
  const root = document.documentElement;
  root.dataset.edition = edition;

  if (edition === "bhh") {
    // Tuerkis fuer BuildMC Horizon Hybrid (Rust + Fabric).
    root.style.setProperty("--accent-h", "182");
    root.style.setProperty("--accent-s", "62%");
    root.style.setProperty("--accent-l", "44%");
    return;
  }

  if (edition === "rust") {
    // Rost-Orange fuer die Rust-Edition.
    root.style.setProperty("--accent-h", "22");
    root.style.setProperty("--accent-s", "78%");
    root.style.setProperty("--accent-l", "52%");
    return;
  }

  if (edition === "bedrock") {
    // Fester, vom eigenen Akzent unabhaengiger Bedrock-Ton.
    root.style.setProperty("--accent-h", "142");
    root.style.setProperty("--accent-s", "55%");
    root.style.setProperty("--accent-l", "48%");
    return;
  }

  const hsl = hexToHsl(c.accent_color);
  if (hsl) {
    root.style.setProperty("--accent-h", String(hsl.h));
    root.style.setProperty("--accent-s", `${Math.max(hsl.s, 55)}%`);
    root.style.setProperty("--accent-l", `${Math.min(Math.max(hsl.l, 52), 70)}%`);
  }
}
