import { useEffect, useLayoutEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { watchLog } from "../lib/liveLog";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { ProjectIcon } from "../components/Avatar";
import { Badge, Button, Empty, Meter, Segmented, Stat, Switch } from "../components/ui";
import { LOADER_LABEL, ago, bytes, num, playtime } from "../lib/format";
import type { CrashDiagnosis, CrashReport, InstalledMod, Snapshot, Telemetry } from "../lib/types";
import "./instance.css";

type Tab = "mods" | "konsole" | "diagnose" | "leistung" | "sicherungen" | "einstellungen";

export function InstanceDetail({ id }: { id: string }) {
  const inst = useApp((s) => s.instances.find((i) => i.id === id));
  const running = useApp((s) => s.running.some((r) => r.instance_id === id));
  const go = useApp((s) => s.go);
  const notify = useApp((s) => s.notify);
  const startInstance = useApp((s) => s.startInstance);
  const stopInstance = useApp((s) => s.stopInstance);
  const [tab, setTab] = useState<Tab>("mods");

  /* .mrpack für die Modrinth App: Mods als Verweis oder mitgepackt, dazu Einstellungen und Konfiguration. */
  async function exportModrinth() {
    try {
      const path = await api.modpacks.exportModrinth(id);
      notify({ kind: "ok", title: "Für Modrinth exportiert", detail: `${path} — in der Modrinth App: Instanz hinzufügen → Aus Datei importieren.` });
    } catch (e) {
      notify({ kind: "risk", title: "Export fehlgeschlagen", detail: String(e) });
    }
  }

  if (!inst) {
    return <Empty icon="warn" title="Instanz nicht gefunden"
                  action={<Button onClick={() => go({ name: "library" })}>Zur Bibliothek</Button>} />;
  }

  return (
    <>
      <header className="topbar">
        <button className="back" onClick={() => go({ name: "library" })} aria-label="Zurück">
          <Icon name="chevron" size={17} />
        </button>
        <div className="topbar__lead">
          <h1 className="topbar__title u-truncate">{inst.name}</h1>
          <Badge tone="accent">{inst.mc_version}</Badge>
          <Badge>{LOADER_LABEL[inst.loader]}</Badge>
          {running && <Badge tone="ok"><span className="live" />Läuft</Badge>}
        </div>

        <div className="topbar__spacer" />

        <Button icon="folder" onClick={() => void api.instances.openFolder(id)}>Ordner</Button>
        <Button icon="folder" onClick={() => void exportModrinth()}>Für Modrinth exportieren</Button>
        <Button tone={running ? "outline" : "primary"} icon={running ? "stop" : "play"}
                onClick={() => void (running ? stopInstance(id) : startInstance(id))}>
          {running ? "Beenden" : "Spielen"}
        </Button>
      </header>

      <div className="inst__stats">
        <Stat label="Spielzeit" value={playtime(inst.play_seconds ?? 0)} />
        <Stat label="Starts" value={num(inst.launch_count ?? 0)} />
        <Stat label="Mods" value={num(inst.mods.length)} />
        <Stat label="Zuletzt" value={ago(inst.last_played)} />
        <Stat label="Arbeitsspeicher" value={`${(inst.max_ram / 1024).toFixed(1)} GB`}
              sub={`min. ${(inst.min_ram / 1024).toFixed(1)} GB`} />
      </div>

      <nav className="inst__tabs">
        <Segmented
          label="Bereich" value={tab} onChange={setTab}
          options={[
            { value: "mods",           label: "Mods", icon: "puzzle" },
            { value: "konsole",        label: "Konsole", icon: "list" },
            { value: "diagnose",       label: "Diagnose", icon: "warn" },
            { value: "leistung",       label: "Leistung", icon: "chip" },
            { value: "sicherungen",    label: "Sicherungen", icon: "camera" },
            { value: "einstellungen",  label: "Einstellungen", icon: "gear" },
          ]}
        />
      </nav>

      <div className="inst u-scroll">
        {tab === "mods"          && <Mods id={id} notify={notify} />}
        {tab === "konsole"       && <Console id={id} running={running} />}
        {tab === "diagnose"      && <Diagnose id={id} />}
        {tab === "leistung"      && <Performance id={id} running={running} />}
        {tab === "sicherungen"   && <Snapshots id={id} notify={notify} />}
        {tab === "einstellungen" && <InstanceSettings id={id} />}
      </div>
    </>
  );
}

/* ---- Mods ------------------------------------------------------------ */

function Mods({ id, notify }: { id: string; notify: (t: any) => void }) {
  const [mods, setMods] = useState<InstalledMod[] | null>(null);
  const [filter, setFilter] = useState("");

  const load = () => void api.mods.installed(id).then(setMods);
  useEffect(load, [id]);

  if (!mods) return <div className="inst__load"><span className="btn__spin" /></div>;
  if (!mods.length) {
    return <Empty icon="puzzle" title="Keine Mods installiert"
                  detail="Unter „Entdecken“ findest du Mods für diese Instanz." />;
  }

  const shown = filter
    ? mods.filter((m) => m.name.toLowerCase().includes(filter.toLowerCase()))
    : mods;
  const outdated = mods.filter((m) => m.update_available);

  return (
    <section className="pane">
      <div className="pane__bar">
        <input className="input" type="search" placeholder="Mods filtern…"
               value={filter} onChange={(e) => setFilter(e.target.value)}
               aria-label="Mods filtern" />
        {outdated.length > 0 && (
          <Button tone="primary" size="sm" icon="refresh"
                  onClick={() => void api.mods.updateAll(id).then(() => {
                    notify({ kind: "ok", title: `${outdated.length} Mods aktualisiert` });
                    load();
                  })}>
            {outdated.length} Update{outdated.length === 1 ? "" : "s"}
          </Button>
        )}
      </div>

      <ul className="modlist u-stagger">
        {shown.map((m) => (
          <li className={`mod${m.enabled ? "" : " is-off"}`} key={m.filename}>
            <ProjectIcon title={m.name} url={m.icon_url} size={34} />

            <div className="mod__id">
              <span className="mod__name u-truncate">{m.name}</span>
              <span className="mod__file u-truncate">{m.filename}</span>
            </div>

            <div className="mod__tags">
              {m.update_available && (
                <Badge tone="warn" title={`Neu: ${m.update_available}`}>
                  <Icon name="refresh" size={11} />{m.update_available}
                </Badge>
              )}
              {m.depends_on.length > 0 && (
                <Badge title={`Benötigt: ${m.depends_on.join(", ")}`}>
                  <Icon name="puzzle" size={11} />{m.depends_on.length}
                </Badge>
              )}
              <span className="mod__size">{bytes(m.size_bytes)}</span>
            </div>

            <Switch checked={m.enabled} label={`${m.name} aktivieren`}
                    onChange={() => void api.mods.toggle(id, m.filename).then(load)} />

            <button className="mod__x" aria-label={`${m.name} entfernen`}
                    onClick={() => void api.mods.remove(id, m.filename).then(load)}>
              <Icon name="trash" size={15} />
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* ---- Leistung -------------------------------------------------------- */

function Performance({ id, running }: { id: string; running: boolean }) {
  const live = useApp((s) => s.telemetry);
  const [flags, setFlags] = useState<string[]>([]);
  const [local, setLocal] = useState<Telemetry | null>(null);

  useEffect(() => {
    void api.launch.suggestFlags(id).then(setFlags).catch(() => setFlags([]));
    if (!running) return;
    void api.launch.telemetry(id).then(setLocal).catch(() => {});
  }, [id, running]);

  const t = live?.instance_id === id ? live : local;

  if (!running) {
    return (
      <section className="pane">
        <Empty icon="chip" title="Instanz läuft nicht"
               detail="Live-Werte zu Speicher, CPU und Garbage Collection erscheinen, sobald das Spiel läuft." />
        <FlagBox flags={flags} />
      </section>
    );
  }

  return (
    <section className="pane">
      {t && (
        <>
          <div className="gauges">
            <Gauge label="Arbeitsspeicher" value={t.rss_bytes} max={t.max_bytes}
                   text={`${bytes(t.rss_bytes)} / ${bytes(t.max_bytes)}`} />
            <Gauge label="Prozessorlast" value={t.cpu_percent} max={100}
                   text={`${t.cpu_percent.toFixed(0)} %`} />
          </div>
          <div className="inst__stats inst__stats--inset">
            <Stat label="Laufzeit" value={playtime(t.uptime_seconds)} />
            <Stat label="Zugewiesen" value={bytes(t.max_bytes)} />
          </div>
          <p className="perf__note">
            Gemessen wird der Prozess von außen. Die Java-Halde und die Pausen
            des Garbage Collectors stehen hier nicht — dafür bräuchte es eine
            Anbindung im Spiel selbst.
          </p>
        </>
      )}
      <FlagBox flags={flags} />
    </section>
  );
}

function Gauge({ label, value, max, text }: {
  label: string; value: number; max: number; text: string;
}) {
  const pct = max ? (value / max) * 100 : 0;
  const tone = pct > 88 ? "risk" : pct > 70 ? "warn" : "accent";
  return (
    <div className="gauge">
      <div className="gauge__head">
        <span>{label}</span>
        <strong>{text}</strong>
      </div>
      <Meter value={value} max={max} tone={tone} label={label} />
    </div>
  );
}

function FlagBox({ flags }: { flags: string[] }) {
  if (!flags.length) return null;
  return (
    <div className="flags">
      <h3 className="flags__title">Empfohlene JVM-Schalter</h3>
      <p className="flags__note">
        Abgeleitet aus Arbeitsspeicher, Minecraft-Version und Anzahl der Mods.
      </p>
      <code className="flags__code">{flags.join(" ")}</code>
    </div>
  );
}

/* ---- Sicherungen ----------------------------------------------------- */

function Snapshots({ id, notify }: { id: string; notify: (t: any) => void }) {
  const [list, setList] = useState<Snapshot[] | null>(null);
  const [busy, setBusy] = useState(false);

  const load = () => void api.snapshots.list(id).then(setList).catch(() => setList([]));
  useEffect(load, [id]);

  async function create() {
    setBusy(true);
    try {
      await api.snapshots.create(id, `Manuell ${new Date().toLocaleString("de-DE")}`, true);
      notify({ kind: "ok", title: "Sicherung angelegt" });
      load();
    } finally { setBusy(false); }
  }

  return (
    <section className="pane">
      <div className="pane__bar">
        <p className="pane__lead">
          Eine Sicherung friert Mods, Konfigurationen und wahlweise die Welten ein.
        </p>
        <Button tone="primary" size="sm" icon="camera" busy={busy} onClick={() => void create()}>
          Jetzt sichern
        </Button>
      </div>

      {list && !list.length && (
        <Empty icon="camera" title="Noch keine Sicherung"
               detail="Lege eine an, bevor du größere Änderungen an den Mods vornimmst." />
      )}

      <ul className="snaps u-stagger">
        {list?.map((s) => (
          <li className="snap" key={s.id}>
            <Icon name="camera" size={16} />
            <div className="mod__id">
              <span className="mod__name u-truncate">{s.label}</span>
              <span className="mod__file">
                {ago(s.created_at)} · {bytes(s.size_bytes)}
                {s.includes_worlds ? " · mit Welten" : ""}
                {s.automatic ? " · automatisch" : ""}
              </span>
            </div>
            <Button size="sm" icon="refresh"
                    onClick={() => void api.snapshots.restore(s.id)}>
              Zurücksetzen
            </Button>
            <button className="mod__x" aria-label="Sicherung löschen"
                    onClick={() => void api.snapshots.remove(s.id).then(load)}>
              <Icon name="trash" size={15} />
            </button>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* ---- Instanz-Einstellungen ------------------------------------------- */

function InstanceSettings({ id }: { id: string }) {
  const inst = useApp((s) => s.instances.find((i) => i.id === id))!;
  const reload = useApp((s) => s.reloadInstances);
  const [draft, setDraft] = useState(inst);

  const dirty = JSON.stringify(draft) !== JSON.stringify(inst);

  return (
    <section className="pane pane--form">
      <label className="field">
        <span className="field__label">Name</span>
        <input className="input" value={draft.name}
               onChange={(e) => setDraft({ ...draft, name: e.target.value })} />
      </label>

      <label className="field">
        <span className="field__label">Gruppe</span>
        <input className="input" value={draft.group ?? ""}
               placeholder="z. B. Survival"
               onChange={(e) => setDraft({ ...draft, group: e.target.value || null })} />
      </label>

      <div className="field__row">
        <label className="field">
          <span className="field__label">Arbeitsspeicher min. (MB)</span>
          <input className="input" type="number" step={512} value={draft.min_ram}
                 onChange={(e) => setDraft({ ...draft, min_ram: +e.target.value })} />
        </label>
        <label className="field">
          <span className="field__label">Arbeitsspeicher max. (MB)</span>
          <input className="input" type="number" step={512} value={draft.max_ram}
                 onChange={(e) => setDraft({ ...draft, max_ram: +e.target.value })} />
        </label>
      </div>

      <label className="field">
        <span className="field__label">Zusätzliche JVM-Schalter</span>
        <input className="input" value={draft.jvm_args.join(" ")}
               placeholder="-XX:+UseZGC …"
               onChange={(e) => setDraft({
                 ...draft, jvm_args: e.target.value.split(/\s+/).filter(Boolean),
               })} />
        <span className="field__hint">Leer lassen, um die globalen Schalter zu nutzen.</span>
      </label>

      <label className="field">
        <span className="field__label">Notizen</span>
        <textarea className="input" rows={3} value={draft.notes ?? ""}
                  onChange={(e) => setDraft({ ...draft, notes: e.target.value })} />
      </label>

      <div className="pane__foot">
        <Button tone="ghost" disabled={!dirty} onClick={() => setDraft(inst)}>
          Verwerfen
        </Button>
        <Button tone="primary" icon="check" disabled={!dirty}
                onClick={() => void api.instances.update(draft).then(reload)}>
          Speichern
        </Button>
      </div>
    </section>
  );
}

/* ---- Konsole ---------------------------------------------------------- */

type Level = "alle" | "warn" | "error";

/** Liest die Dringlichkeit aus `[..][Thread/LEVEL]:`. */
function levelOf(line: string): "error" | "warn" | "info" {
  if (/\/(ERROR|FATAL)\]/.test(line)) return "error";
  if (/\/WARN\]/.test(line)) return "warn";
  return "info";
}

function Console({ id, running }: { id: string; running: boolean }) {
  const [lines, setLines] = useState<string[]>([]);
  const [level, setLevel] = useState<Level>("alle");
  const [follow, setFollow] = useState(true);
  const box = useRef<HTMLDivElement>(null);

  const watcher = useRef<ReturnType<typeof watchLog> | null>(null);
  const lastScrollTop = useRef(0);
  const notify = useApp((s) => s.notify);

  useEffect(() => {
    setLines([]);
    setFollow(true);
    lastScrollTop.current = 0;
    const live = watchLog(() => api.launch.log(id), setLines);
    watcher.current = live;
    return () => {
      live.dispose();
      if (watcher.current === live) watcher.current = null;
    };
  }, [id]);

  // Nach dem DOM-Update scrollen, auch beim Wechsel des Logfilters.
  useLayoutEffect(() => {
    const el = box.current;
    if (follow && el) {
      el.scrollTop = el.scrollHeight;
      lastScrollTop.current = el.scrollTop;
    }
  }, [lines, follow, level]);

  async function clearLog() {
    try {
      await watcher.current?.clear(() => api.launch.clearLog(id));
    } catch (e) {
      notify({ kind: "risk", title: "Protokoll konnte nicht geleert werden", detail: String(e) });
    }
  }

  const shown = level === "alle"
    ? lines
    : lines.filter((l) => level === "error"
        ? levelOf(l) === "error"
        : levelOf(l) !== "info");

  const errors = lines.filter((l) => levelOf(l) === "error").length;
  const warns = lines.filter((l) => levelOf(l) === "warn").length;

  return (
    <section className="pane">
      <div className="pane__bar">
        <Segmented
          label="Dringlichkeit" value={level} onChange={setLevel}
          options={[
            { value: "alle",  label: `Alle (${num(lines.length)})` },
            { value: "warn",  label: `Warnungen (${num(warns)})` },
            { value: "error", label: `Fehler (${num(errors)})` },
          ]}
        />
        <div className="topbar__spacer" />
        <label className="con__follow">
          <Switch checked={follow} onChange={setFollow} label="Mitlaufen" />
          <span>Mitlaufen</span>
        </label>
        <Button size="sm" icon="clone"
                onClick={() => void navigator.clipboard?.writeText(lines.join("\n"))}>
          Kopieren
        </Button>
        <Button size="sm" icon="trash"
                onClick={() => void clearLog()}>
          Leeren
        </Button>
      </div>

      {!lines.length ? (
        <Empty icon="list" title="Kein Protokoll"
               detail={running
                 ? "Das Spiel läuft, hat aber noch nichts geschrieben."
                 : "Starte die Instanz, um die Ausgabe hier mitzulesen."} />
      ) : (
        <div className="con" ref={box}
             onScroll={(e) => {
               const el = e.currentTarget;
               const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
               if (atEnd) setFollow(true);
               else if (el.scrollTop < lastScrollTop.current - 1) setFollow(false);
               lastScrollTop.current = el.scrollTop;
             }}>
          {shown.map((line, i) => (
            <div className={`con__line con__line--${levelOf(line)}`} key={i}>
              {line}
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

/* ---- Diagnose --------------------------------------------------------- */

function Diagnose({ id }: { id: string }) {
  const [reports, setReports] = useState<CrashReport[] | null>(null);
  const [live, setLive] = useState<CrashDiagnosis | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    void api.launch.crashes(id).then(setReports).catch(() => setReports([]));
  }, [id]);

  async function analyze() {
    setBusy(true);
    try { setLive(await api.launch.analyze(id)); }
    finally { setBusy(false); }
  }

  const none = reports && !reports.length && !live;

  return (
    <section className="pane">
      <div className="pane__bar">
        <p className="pane__lead">
          Sucht im Protokoll die auslösende Zeile, benennt die Ursache und
          schlägt vor, was zu tun ist.
        </p>
        <Button tone="primary" size="sm" icon="spark" busy={busy}
                onClick={() => void analyze()}>
          Protokoll auswerten
        </Button>
      </div>

      {none && (
        <Empty icon="check" title="Kein Absturz gefunden"
               detail="Im aufgezeichneten Protokoll steht nichts, was auf einen Fehlschlag hindeutet." />
      )}

      {live && <DiagnosisCard d={live} when="Aus dem aktuellen Protokoll" />}

      {reports?.map((r) => (
        <article className="crash" key={r.occurred_at}>
          <header className="crash__head">
            <Badge tone="risk"><Icon name="warn" size={11} />Code {r.exit_code}</Badge>
            <span className="crash__when">{ago(r.occurred_at)}</span>
          </header>

          {r.diagnosis
            ? <DiagnosisCard d={r.diagnosis} />
            : <p className="crash__unknown">
                Die Ursache ließ sich nicht eindeutig bestimmen.
              </p>}

          <details className="crash__raw">
            <summary>Protokollauszug</summary>
            <pre>{r.log_excerpt}</pre>
          </details>
        </article>
      ))}
    </section>
  );
}

function DiagnosisCard({ d, when }: { d: CrashDiagnosis; when?: string }) {
  return (
    <div className="diag u-rise">
      {when && <span className="diag__when">{when}</span>}

      <h3 className="diag__cause">
        <Icon name="warn" size={17} />
        {d.cause}
      </h3>

      {d.culprit_mod && (
        <Badge tone="warn" title="Vermutlich verantwortlich">
          <Icon name="puzzle" size={11} />{d.culprit_mod}
        </Badge>
      )}

      <p className="diag__text">{d.explanation}</p>

      {d.suggestions.length > 0 && (
        <>
          <h4 className="diag__h">Was du versuchen kannst</h4>
          <ol className="diag__steps">
            {d.suggestions.map((s, i) => <li key={i}>{s}</li>)}
          </ol>
        </>
      )}
    </div>
  );
}
