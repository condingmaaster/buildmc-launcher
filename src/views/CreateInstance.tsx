import { useEffect, useMemo, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Badge, Button, Field } from "../components/ui";
import { CHANNEL_LABEL, CHANNEL_HUE, DEFAULT_CHANNELS } from "../lib/versions";
import type { Loader, VersionChannel, VersionEntry } from "../lib/types";
import "./create.css";

const LOADERS: { value: Loader; label: string; detail: string }[] = [
  { value: "fabric",  label: "Fabric",  detail: "Größte Mod-Auswahl, kurz nach jeder neuen Version verfügbar." },
  { value: "quilt",   label: "Quilt",   detail: "Abspaltung von Fabric. Nimmt die meisten Fabric-Mods an." },
  { value: "vanilla", label: "Ohne",    detail: "Reines Minecraft. Mods lassen sich später nicht nachrüsten." },
];

export function CreateInstance({ onClose }: { onClose: () => void }) {
  const notify = useApp((s) => s.notify);
  const reload = useApp((s) => s.reloadInstances);
  const go = useApp((s) => s.go);

  const [versions, setVersions] = useState<VersionEntry[]>([]);
  const [channels, setChannels] = useState<VersionChannel[]>(DEFAULT_CHANNELS);
  const [filter, setFilter] = useState("");
  const [picked, setPicked] = useState("");
  const [loader, setLoader] = useState<Loader>("fabric");
  const [name, setName] = useState("");
  const [busy, setBusy] = useState(false);
  const [step, setStep] = useState("");

  useEffect(() => {
    void api.manifest.versions()
      .then((v) => {
        setVersions(v);
        // Die neueste Ausgabe ist die naheliegende Vorauswahl.
        const neueste = v.find((e) => e.channel === "release");
        if (neueste) setPicked(neueste.id);
      })
      .catch(() => setVersions([]));
  }, []);

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape" && !busy) onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  const shown = useMemo(() => {
    const q = filter.trim().toLowerCase();
    return versions
      .filter((v) => channels.includes(v.channel))
      .filter((v) => !q || v.id.toLowerCase().includes(q))
      .slice(0, 400);
  }, [versions, channels, filter]);

  /* Loader, die es für die gewählte Version überhaupt gibt. */
  const moeglich = useMemo(() => {
    const e = versions.find((v) => v.id === picked);
    return e?.available_loaders;
  }, [versions, picked]);

  async function create() {
    if (!picked || busy) return;
    setBusy(true);
    try {
      setStep("Instanz wird angelegt…");
      const instance = await api.instances.create(
        name.trim() || `${picked}${loader === "vanilla" ? "" : ` ${loader}`}`,
        picked,
        loader,
      );

      // Ohne installierten Loader startet die Instanz als Vanilla — genau
      // der Fehler, der hier schon einmal für Verwirrung gesorgt hat.
      if (loader !== "vanilla") {
        setStep(`${loader} wird installiert…`);
        const fassungen = await api.manifest.loaderVersions(loader, picked);
        // Erst die als "stabil" markierte, sonst wenigstens die erste --
        // nie das ganze Objekt weiterreichen, das Backend will nur die
        // Versionskennung als Zeichenkette.
        const neueste = fassungen.find((f) => f.stable) ?? fassungen[0];
        if (!neueste) {
          throw new Error(`Für ${picked} gibt es kein ${loader}.`);
        }
        await api.manifest.installLoader(instance.id, loader, neueste.version);
      }

      await reload();
      notify({ kind: "ok", title: `${instance.name} angelegt` });
      onClose();
      go({ name: "instance", id: instance.id });
    } catch (e) {
      notify({ kind: "risk", title: "Anlegen fehlgeschlagen", detail: String(e) });
    } finally {
      setBusy(false);
      setStep("");
    }
  }

  const toggleChannel = (c: VersionChannel) =>
    setChannels((prev) => (prev.includes(c) ? prev.filter((x) => x !== c) : [...prev, c]));

  return (
    <div className="sheet__scrim" onClick={() => !busy && onClose()}>
      <div className="create" role="dialog" aria-label="Neue Instanz"
           onClick={(e) => e.stopPropagation()}>
        <header className="create__head">
          <h2>Neue Instanz</h2>
          <button className="sheet__x" onClick={onClose} disabled={busy}
                  aria-label="Schließen">
            <Icon name="close" size={16} />
          </button>
        </header>

        <div className="create__body u-scroll">
          <Field label="Name" hint="Leer lassen für einen Vorschlag aus Version und Loader.">
            <input className="input" value={name} autoFocus
                   placeholder={picked ? `${picked} ${loader}` : "…"}
                   onChange={(e) => setName(e.target.value)} />
          </Field>

          <div className="create__section">
            <span className="field__label">Mod-Loader</span>
            <div className="create__loaders">
              {LOADERS.map((l) => {
                const fehlt = l.value !== "vanilla"
                  && moeglich !== undefined && !moeglich.includes(l.value);
                return (
                  <button key={l.value} disabled={fehlt}
                          className={`qopt${loader === l.value ? " is-on" : ""}`}
                          onClick={() => setLoader(l.value)}>
                    <span className="qopt__mark">
                      {loader === l.value && <Icon name="check" size={13} />}
                    </span>
                    <span className="qopt__text">
                      <strong>{l.label}</strong>
                      <span>{fehlt ? `Für ${picked} nicht verfügbar.` : l.detail}</span>
                    </span>
                  </button>
                );
              })}
            </div>
          </div>

          <div className="create__section">
            <div className="create__vhead">
              <span className="field__label">Version</span>
              <input className="input create__search" type="search" value={filter}
                     placeholder="filtern…"
                     onChange={(e) => setFilter(e.target.value)} />
            </div>

            {/* Alle Kanäle zuschaltbar — auch Beta, Alpha und Aprilscherze. */}
            <div className="create__channels">
              {(Object.keys(CHANNEL_LABEL) as VersionChannel[]).map((c) => (
                <button key={c} onClick={() => toggleChannel(c)}
                        className={`chan${channels.includes(c) ? " is-on" : ""}`}
                        style={{ "--chan-h": String(CHANNEL_HUE[c]) } as React.CSSProperties}>
                  {CHANNEL_LABEL[c]}
                </button>
              ))}
            </div>

            <div className="create__versions u-scroll">
              {shown.length === 0 && (
                <p className="create__none">
                  {versions.length === 0
                    ? "Versionsliste wird geladen…"
                    : "Keine Version passt zu Filter und Kanälen."}
                </p>
              )}
              {shown.map((v) => (
                <button key={v.id} onClick={() => setPicked(v.id)}
                        className={`vrow${picked === v.id ? " is-on" : ""}`}>
                  <span className="vrow__id">{v.id}</span>
                  <Badge hue={CHANNEL_HUE[v.channel]}>{CHANNEL_LABEL[v.channel]}</Badge>
                  {v.installed && <Badge tone="ok">geladen</Badge>}
                  <span className="vrow__date">{v.released_at.slice(0, 10)}</span>
                </button>
              ))}
            </div>
          </div>
        </div>

        <footer className="create__foot">
          {step && <span className="create__step">{step}</span>}
          <Button tone="ghost" onClick={onClose} disabled={busy}>Abbrechen</Button>
          <Button tone="primary" icon="plus" busy={busy} disabled={!picked}
                  onClick={() => void create()}>
            Anlegen
          </Button>
        </footer>
      </div>
    </div>
  );
}
