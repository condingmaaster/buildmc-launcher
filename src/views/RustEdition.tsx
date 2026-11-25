/* =========================================================================
   Rust-Editionen -- Minecraft: Java Edition 26.3, in Rust neu geschrieben.

   variant="horizon"  Horizon (MIT), unveraendert (Option 3)
   variant="bhh"      BuildMC Horizon Hybrid: Horizon + Fabric-Bruecke (Option 4)

   BuildMC enthaelt den Quellcode beider, baut ihn selbst, bereitet die
   Spieldaten aus der eigenen 26.3 vor und startet das Spiel ohne fremden
   Launcher. Drei Schritte, jeder mit sichtbarem Fortschritt:
     1. Bauen        (cargo, einmalig 20-40 min)
     2. Vorbereiten  (26.3 + Java 25 laden, horizon-prepare)
     3. Spielen      (eigene Instanzen, Konto ueber stdin)
   BHH zusaetzlich: Fabric-Mods je Instanz mit Kompatibilitaetspruefung.
   ========================================================================= */

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Badge, Button, Empty } from "../components/ui";
import { ago, playtime } from "../lib/format";
import type { FabricMod, Progress, RustInstance, RustStatus, RustVariant } from "../lib/types";
import { DeviceCode, type DeviceLogin } from "./Settings";
import "./bedrock.css";
import "./rust.css";

const TITEL: Record<RustVariant, string> = {
  horizon: "Horizon",
  bhh: "BuildMC Horizon Hybrid",
};

export function RustEdition({ variant }: { variant: RustVariant }) {
  const notify = useApp((s) => s.notify);
  const running = useApp((s) => s.rustRunning);
  const reloadAll = useApp((s) => s.reloadRustInstances);
  const startInstance = useApp((s) => s.startRustInstance);
  const stopInstance = useApp((s) => s.stopRustInstance);

  const [status, setStatus] = useState<RustStatus | null>(null);
  const [instances, setInstances] = useState<RustInstance[]>([]);
  const [buildLines, setBuildLines] = useState<string[]>([]);
  const [buildFraction, setBuildFraction] = useState(0);
  const [prep, setPrep] = useState<Progress | null>(null);
  const [prepLog, setPrepLog] = useState<string[]>([]);
  const [startedAt, setStartedAt] = useState<number | null>(null);
  const [now, setNow] = useState(Date.now());
  const [newName, setNewName] = useState("");
  const [newServer, setNewServer] = useState("");
  const [busy, setBusy] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const logRef = useRef<HTMLPreElement>(null);
  // Horizon startet ohne den offiziellen Launcher und braucht darum die eigene Anmeldung.
  const eigeneAnmeldung = useApp((s) => !!s.config?.own_login);
  const reloadAccounts = useApp((s) => s.reloadAccounts);
  const [device, setDevice] = useState<DeviceLogin | null>(null);

  async function anmelden() {
    try {
      setDevice(await api.auth.startDevice());
    } catch (e) {
      notify({ kind: "risk", title: "Anmeldung nicht möglich", detail: String(e) });
    }
  }

  const loadStatus = useCallback(() => void api.rust.status(variant).then(setStatus).catch(() => setStatus(null)), [variant]);
  const loadInstances = useCallback(async () => {
    setInstances(await api.rust.listInstances(variant).catch(() => []));
    void reloadAll();
  }, [variant, reloadAll]);

  useEffect(() => {
    loadStatus();
    void loadInstances();
    const offs = [
      api.events.rustBuild((e) => {
        if (e.variant !== variant) return;
        if (e.line) setBuildLines((l) => [...l.slice(-199), e.line]);
        setBuildFraction(e.fraction);
        if (e.done) {
          loadStatus();
          if (e.error) notify({ kind: "risk", title: "Bau fehlgeschlagen", detail: e.error });
          else notify({ kind: "ok", title: `${TITEL[variant]} ist gebaut`, detail: "Jetzt die Spieldaten vorbereiten." });
        }
      }),
      api.events.rustPrepareProgress((p) => {
        setPrep(p);
        const zeile = p.detail ? `${p.label} · ${p.detail}` : p.label;
        setPrepLog((l) => (l[l.length - 1] === zeile ? l : [...l.slice(-199), zeile]));
        if (p.done) {
          loadStatus();
          if (p.error) notify({ kind: "risk", title: "Vorbereitung fehlgeschlagen", detail: p.error });
        }
      }),
    ];
    return () => { offs.forEach((o) => void o.then((f) => f())); };
  }, [variant, loadStatus, loadInstances, notify]);

  useEffect(() => { logRef.current?.scrollTo(0, logRef.current.scrollHeight); }, [buildLines, prepLog]);

  // Laufzeit sichtbar machen: auch in Phasen ohne Fortschrittsmeldung sieht man, dass es laeuft.
  const busyNow = !!status && (status.building || status.preparing);
  useEffect(() => {
    if (!busyNow) return;
    setStartedAt((t) => t ?? Date.now());
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [busyNow]);
  useEffect(() => { if (!busyNow) setStartedAt(null); }, [busyNow]);
  const laufzeit = startedAt ? Math.max(0, Math.floor((now - startedAt) / 1000)) : 0;
  const uhr = `${Math.floor(laufzeit / 60)}:${String(laufzeit % 60).padStart(2, "0")}`;

  async function build() {
    setBuildLines([]);
    setStartedAt(Date.now());
    setBuildFraction(0);
    setStatus((s) => (s ? { ...s, building: true } : s));
    try { await api.rust.build(variant); } catch { /* Meldung kommt ueber das Ereignis */ }
  }

  async function prepare() {
    setPrep(null);
    setPrepLog([]);
    setStartedAt(Date.now());
    setStatus((s) => (s ? { ...s, preparing: true } : s));
    try { await api.rust.prepare(variant); } catch { /* Meldung kommt ueber das Ereignis */ }
  }

  async function createInstance(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim()) return;
    setBusy(true);
    try {
      await api.rust.createInstance(variant, newName.trim(), newServer.trim() || undefined);
      notify({ kind: "ok", title: `Instanz „${newName.trim()}“ angelegt` });
      setNewName("");
      setNewServer("");
      await loadInstances();
    } catch (err) {
      notify({ kind: "risk", title: "Anlegen fehlgeschlagen", detail: String(err) });
    } finally { setBusy(false); }
  }

  async function remove(id: string, name: string) {
    if (!window.confirm(`„${name}“ mit allen Welten löschen?`)) return;
    try {
      await api.rust.deleteInstance(variant, id);
      await loadInstances();
    } catch (err) {
      notify({ kind: "risk", title: "Löschen fehlgeschlagen", detail: String(err) });
    }
  }

  if (!status) return null;

  const bereit = status.built && status.prepared && !status.outdated;
  const prepPct = prep && prep.total > 0 ? prep.current / prep.total : 0;

  return (
    <>
      <header className="topbar">
        <div className="topbar__lead"><h1 className="topbar__title">{TITEL[variant]}</h1></div>
      </header>

      <div className="bed u-scroll">
        {variant === "bhh" && (
          <p className="bed__note rust__intro">
            Horizons native Rust-Engine plus eine eingebettete JVM für Fabric-Mods. Die Fabric-Unterstützung wächst
            Schritt für Schritt: welcher Mod schon läuft, prüft BuildMC an jeder Mod-Datei selbst (siehe Instanzen).
          </p>
        )}

        <section className="bed__card">
          <h2 className="bed__h">Status</h2>
          <div className="bed__status-row">
            {status.bundled && !status.source_found ? (
              <Badge tone="ok">Mitgeliefert</Badge>
            ) : (
              <Badge tone={status.source_found ? "ok" : "neutral"}>
                {status.source_found ? `Quellcode ${status.source_revision.slice(0, 7)}` : "Quellcode fehlt"}
              </Badge>
            )}
            <Badge tone={status.built && !status.outdated ? "ok" : "neutral"}>
              {status.built ? (status.outdated ? "Gebaut (veraltet)" : `Gebaut ${status.horizon_version}`) : "Nicht gebaut"}
            </Badge>
            <Badge tone={status.prepared ? "ok" : "neutral"}>
              {status.prepared ? `Spieldaten ${status.minecraft_version} bereit` : "Spieldaten fehlen"}
            </Badge>
            <Badge tone={status.microsoft_account ? "ok" : "neutral"}>
              {status.microsoft_account ? `Konto ${status.account_name}` : "Kein Microsoft-Konto"}
            </Badge>
            {variant === "bhh" && (
              <Badge tone={status.jdk_found ? "ok" : "neutral"}>{status.jdk_found ? "JDK 25 vorhanden" : "JDK 25 fehlt"}</Badge>
            )}
          </div>

          {!status.microsoft_account && (
            <p className="bed__note">
              {eigeneAnmeldung ? (
                <>
                  Startet nur mit einem Microsoft-Konto, dem Minecraft: Java Edition gehört.{" "}
                  <Button size="sm" tone="primary" icon="plus" onClick={() => void anmelden()}>Mit Microsoft anmelden</Button>
                </>
              ) : (
                <>
                  Horizon startet ohne den offiziellen Minecraft-Launcher und braucht darum die eigene
                  Microsoft-Anmeldung. Die ist aus, bis Mojang die Anwendung freigibt (Einstellungen → Konten).
                </>
              )}
            </p>
          )}
          {!status.cargo_found && status.source_found && (
            <p className="bed__note">
              Zum Bauen fehlt <code>cargo</code>. Rust installieren:{" "}
              <a href="#" onClick={(e) => { e.preventDefault(); void api.system.openExternal("https://rustup.rs"); }}>rustup.rs</a>
            </p>
          )}
          {variant === "bhh" && !status.jdk_found && !status.bundled && (
            <p className="bed__note">
              BHH braucht ein JDK 25 für die eingebettete JVM. In der Java-Ansicht unter Einstellungen → Java einmal
              Java 25 installieren.
            </p>
          )}

          <div className="bed__actions">
            {status.source_found && (!status.built || status.outdated) && (
              <Button tone="primary" icon="download" busy={status.building}
                      disabled={!status.source_found || !status.cargo_found || status.building || (variant === "bhh" && !status.jdk_found)}
                      onClick={() => void build()}>
                {status.built ? "Neu bauen" : `${TITEL[variant]} bauen`}
              </Button>
            )}
            {status.built && !status.outdated && !status.prepared && (
              <Button tone="primary" icon="download" busy={status.preparing}
                      disabled={!status.microsoft_account || status.preparing}
                      onClick={() => void prepare()}>
                Spieldaten vorbereiten
              </Button>
            )}
            {status.source_found && status.built && status.prepared && (
              <Button tone="ghost" icon="refresh" busy={status.building} disabled={status.building}
                      onClick={() => void build()}>Neu bauen</Button>
            )}
            <Button tone="ghost" icon="refresh" onClick={loadStatus}>Neu prüfen</Button>
          </div>

          {(status.building || buildLines.length > 0) && (
            <div className="rust__job">
              <div className={`rust__bar${status.building ? " is-busy" : ""}`}><span style={{ width: `${Math.round(buildFraction * 100)}%` }} /></div>
              <span className="rust__job-label">
                {status.building ? `Bauen … ${Math.round(buildFraction * 100)} % · läuft seit ${uhr} · beim ersten Mal 20–40 Minuten` : "Bau-Ausgabe"}
              </span>
              <pre className="rust__log" ref={logRef}>{buildLines.slice(-60).join("\n")}</pre>
            </div>
          )}

          {(status.preparing || prepLog.length > 0) && (
            <div className="rust__job">
              <div className={`rust__bar${status.preparing ? " is-busy" : ""}`}><span style={{ width: `${Math.round(prepPct * 100)}%` }} /></div>
              <span className="rust__job-label">
                {prep?.label ?? "Vorbereiten …"}{prep?.detail ? ` · ${prep.detail}` : ""}
                {status.preparing && ` · läuft seit ${uhr} · insgesamt etwa 5–15 Minuten`}
              </span>
              {prepLog.length > 0 && <pre className="rust__log" ref={logRef}>{prepLog.slice(-60).join("\n")}</pre>}
            </div>
          )}
        </section>

        <section className="bed__card">
          <h2 className="bed__h">Instanz anlegen</h2>
          <form className="bed__create" onSubmit={(e) => void createInstance(e)}>
            <input className="input" placeholder="Name der Instanz" value={newName}
                   onChange={(e) => setNewName(e.target.value)} maxLength={32} />
            <input className="input" placeholder="Server beim Start (optional), z. B. lobby.cousinsmp.de"
                   value={newServer} onChange={(e) => setNewServer(e.target.value)} maxLength={120} />
            <Button tone="outline" icon="plus" type="submit" busy={busy} disabled={!newName.trim()}>
              Anlegen
            </Button>
          </form>
        </section>

        <section className="bed__card">
          <h2 className="bed__h">Instanzen</h2>
          {instances.length === 0 ? (
            <Empty icon={variant === "bhh" ? "hybrid" : "rust"} title="Keine Instanz" detail="Oben eine anlegen, um loszuspielen." />
          ) : (
            <ul className="bed__list">
              {instances.map((inst) => {
                const isRunning = running.some((r) => r.instance_id === inst.id);
                return (
                  <li key={inst.id} className="rust__entry">
                    <div className={`bed__item${isRunning ? " is-running" : ""}`}>
                      <Icon name={variant === "bhh" ? "hybrid" : "rust"} size={22} />
                      <div className="mod__id">
                        <span className="mod__name">{inst.name}</span>
                        <span className="mod__file">
                          {inst.quick_server ? `→ ${inst.quick_server} · ` : ""}
                          {playtime(inst.play_seconds)} ·{" "}
                          {inst.last_played ? `zuletzt ${ago(inst.last_played)}` : "noch nie gespielt"}
                        </span>
                      </div>
                      {isRunning && <Badge tone="ok"><span className="live" />Läuft</Badge>}
                      {variant === "bhh" && (
                        <Button tone="ghost" icon="puzzle" onClick={() => setOpen(open === inst.id ? null : inst.id)}>
                          Fabric-Mods
                        </Button>
                      )}
                      {!isRunning && (
                        <Button tone="ghost" icon="trash" onClick={() => void remove(inst.id, inst.name)}
                                aria-label={`${inst.name} löschen`} />
                      )}
                      <Button tone={isRunning ? "outline" : "primary"} icon={isRunning ? "stop" : "play"}
                              disabled={!isRunning && !bereit}
                              title={!bereit ? "Erst bauen und vorbereiten" : undefined}
                              onClick={() => void (isRunning ? stopInstance(inst.id) : startInstance(inst.id, variant))}>
                        {isRunning ? "Beenden" : "Spielen"}
                      </Button>
                    </div>
                    {variant === "bhh" && open === inst.id && (
                      <FabricPanel instance={inst} onChanged={() => void loadInstances()} />
                    )}
                  </li>
                );
              })}
            </ul>
          )}
        </section>

        <p className="bed__note rust__legal">
          {variant === "bhh" ? "BuildMC Horizon Hybrid ist ein Fork von " : "Auf Basis von "}
          <a href="#" onClick={(e) => { e.preventDefault(); void api.system.openExternal("https://github.com/horizon-rust/horizon"); }}>Horizon</a>{" "}
          (MIT, © 2026 Ludovic Marie and the Horizon contributors). Inoffiziell -- nicht von Mojang oder Microsoft
          {variant === "bhh" ? ", nicht von den Horizon- oder Fabric-Entwicklern" : ""}.
          Enthält keine Minecraft-Dateien: die Spieldaten entstehen aus deiner eigenen, gekauften Kopie.
        </p>
      </div>
      {device && (
        <DeviceCode
          start={device}
          onDone={async (konto) => {
            setDevice(null);
            await reloadAccounts();
            loadStatus();
            notify({ kind: "ok", title: `Angemeldet als ${konto.username}` });
          }}
          onCancel={() => setDevice(null)}
        />
      )}
    </>
  );
}

/** BHH: Fabric-Mods einer Instanz, jede mit Pruefergebnis aus ihrem Bytecode. */
function FabricPanel({ instance, onChanged }: { instance: RustInstance; onChanged: () => void }) {
  const notify = useApp((s) => s.notify);
  const [mods, setMods] = useState<FabricMod[] | null>(null);
  const [heap, setHeap] = useState(instance.jvm_heap_mb);

  const load = useCallback(() => void api.bhh.listMods(instance.id).then(setMods).catch((e) => {
    setMods([]);
    notify({ kind: "risk", title: "Mods ließen sich nicht lesen", detail: String(e) });
  }), [instance.id, notify]);

  useEffect(() => { load(); }, [load]);

  async function saveHeap() {
    try {
      await api.rust.updateInstance("bhh", instance.id, { jvmHeapMb: heap });
      onChanged();
      notify({ kind: "ok", title: `JVM-Speicher: ${heap} MB` });
    } catch (e) {
      notify({ kind: "risk", title: "Speichern fehlgeschlagen", detail: String(e) });
    }
  }

  async function addMod() {
    try {
      await api.bhh.pickMods(instance.id);
      load();
    } catch (e) {
      notify({ kind: "risk", title: "Hinzufügen fehlgeschlagen", detail: String(e) });
    }
  }

  async function removeMod(file: string) {
    try {
      await api.bhh.removeMod(instance.id, file);
      load();
    } catch (e) {
      notify({ kind: "risk", title: "Entfernen fehlgeschlagen", detail: String(e) });
    }
  }

  const bereit = mods?.filter((m) => m.status === "bereit").length ?? 0;

  return (
    <div className="rust__mods">
      <div className="bed__actions">
        <Button tone="outline" icon="plus" onClick={() => void addMod()}>Mod hinzufügen</Button>
        <Button tone="ghost" icon="folder" onClick={() => void api.bhh.openModsDir(instance.id)}>Mod-Ordner öffnen</Button>
        <Button tone="ghost" icon="refresh" onClick={load}>Neu prüfen</Button>
        <label className="rust__heap">
          JVM-Speicher
          <input className="input" type="number" min={128} max={8192} step={128} value={heap}
                 onChange={(e) => setHeap(Number(e.target.value))} onBlur={() => void saveHeap()} />
          MB
        </label>
      </div>
      {mods === null ? null : mods.length === 0 ? (
        <p className="bed__note">Noch keine Fabric-Mods. Ohne Mods startet BHH keine JVM und ist so schnell wie Horizon.</p>
      ) : (
        <>
          <p className="bed__note">{bereit} von {mods.length} Mod(s) bereit -- geprüft an den Klassen, die jeder Mod wirklich benutzt.</p>
          <ul className="bed__list">
            {mods.map((m) => (
              <li key={m.file} className="rust__mod">
                <div className="rust__mod-head">
                  <Badge tone={m.status === "bereit" ? "ok" : m.status === "kein_fabric_mod" ? "risk" : "warn"}>
                    {m.status === "bereit" ? "Bereit" : m.status === "kein_fabric_mod" ? "Kein Fabric-Mod" : "Unvollständig"}
                  </Badge>
                  <span className="mod__name">{m.name || m.file}</span>
                  <span className="mod__file">{m.version} · {m.used_classes} benutzte Klassen</span>
                  <Button tone="ghost" icon="trash" onClick={() => void removeMod(m.file)} aria-label={`${m.file} entfernen`} />
                </div>
                {m.reasons.length > 0 && (
                  <ul className="rust__reasons">
                    {m.reasons.map((r) => <li key={r}>{r}</li>)}
                  </ul>
                )}
                {m.missing_classes.length > 0 && (
                  <details className="rust__missing">
                    <summary>Fehlende Klassen ({m.missing_classes.length})</summary>
                    <pre>{m.missing_classes.map((c) => c.replaceAll("/", ".")).join("\n")}</pre>
                  </details>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}
