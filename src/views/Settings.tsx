import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Avatar } from "../components/Avatar";
import { Badge, Button, Empty, Field, Segmented, Switch } from "../components/ui";
import type { Account, AiModel, AiStatus } from "../lib/types";
import "./settings.css";

type Tab = "konten" | "spiel" | "darstellung" | "assistent" | "erweitert";

export function Settings() {
  const [tab, setTab] = useState<Tab>("konten");
  return (
    <>
      <header className="topbar">
        <div className="topbar__lead"><h1 className="topbar__title">Einstellungen</h1></div>
        <div className="topbar__spacer" />
        <Segmented
          label="Bereich" value={tab} onChange={setTab}
          options={[
            { value: "konten",      label: "Konten" },
            { value: "spiel",       label: "Spiel" },
            { value: "darstellung", label: "Darstellung" },
            { value: "assistent",   label: "Assistent" },
            { value: "erweitert",   label: "Erweitert" },
          ]}
        />
      </header>

      <div className="set u-scroll">
        {tab === "konten"      && <Accounts />}
        {tab === "spiel"       && <Game />}
        {tab === "darstellung" && <Look />}
        {tab === "assistent"   && <Assistant />}
        {tab === "erweitert"   && <Advanced />}
      </div>
    </>
  );
}

/* ---- Konten ---------------------------------------------------------- */

function Accounts() {
  const accounts = useApp((s) => s.accounts);
  const notify = useApp((s) => s.notify);
  const reload = useApp((s) => s.reloadAccounts);
  const [offlineName, setOfflineName] = useState("");
  const [busy, setBusy] = useState(false);
  const [device, setDevice] = useState<DeviceLogin | null>(null);

  /* Jede Aenderung muss die Liste neu laden. Ohne das aendert sich an der
     Oberflaeche nichts und die Knoepfe wirken wirkungslos. */
  async function run(label: string, fn: () => Promise<unknown>) {
    setBusy(true);
    try {
      await fn();
      await reload();
    } catch (e) {
      notify({ kind: "risk", title: label, detail: String(e) });
    } finally { setBusy(false); }
  }

  async function addOffline(e: React.FormEvent) {
    e.preventDefault();
    const n = offlineName.trim();
    if (!n) return;
    await run("Konto konnte nicht angelegt werden", async () => {
      await api.auth.addOffline(n);
      setOfflineName("");
      notify({ kind: "ok", title: `Offline-Konto „${n}“ angelegt` });
    });
  }

  async function startMicrosoft() {
    setBusy(true);
    try {
      const start = await api.auth.startDevice();
      setDevice(start);
    } catch (e) {
      notify({ kind: "risk", title: "Anmeldung nicht m\u00f6glich", detail: String(e) });
    } finally { setBusy(false); }
  }

  return (
    <section className="pane pane--form">
      <h2 className="set__h">Angemeldete Konten</h2>

      {!accounts.length && (
        <Empty icon="spark" title="Kein Konto"
               detail="Melde dich mit Microsoft an, um auf öffentlichen Servern zu spielen." />
      )}

      <ul className="acc__list">
        {accounts.map((a) => (
          <li className={`acc${a.active ? " is-active" : ""}`} key={a.id}>
            <Avatar name={a.username} uuid={a.uuid} skinUrl={a.skin_url} size={38} />
            <div className="mod__id">
              <span className="mod__name">{a.username}</span>
              <span className="mod__file">{a.uuid}</span>
            </div>
            <Badge tone={a.account_type === "offline" ? "warn" : "ok"}>
              {a.account_type === "offline" ? "Offline" : "Microsoft"}
            </Badge>
            {a.active && <Badge tone="accent">aktiv</Badge>}
            {!a.active && (
              <Button size="sm" disabled={busy}
                      onClick={() => void run("Konto konnte nicht aktiviert werden",
                                              () => api.auth.activate(a.id))}>
                Verwenden
              </Button>
            )}
            <button className="mod__x" aria-label={`${a.username} entfernen`}
                    disabled={busy}
                    onClick={() => void run("Konto konnte nicht entfernt werden",
                                            () => api.auth.remove(a.id))}>
              <Icon name="trash" size={15} />
            </button>
          </li>
        ))}
      </ul>

      <div className="acc__add">
        <Button tone="primary" icon="plus" busy={busy}
                onClick={() => void startMicrosoft()}>
          Microsoft-Konto hinzufügen
        </Button>
      </div>

      <h2 className="set__h">Offline-Konto</h2>
      <p className="set__note">
        Ein Offline-Konto braucht keine Anmeldung. Die UUID wird fest aus dem Namen
        abgeleitet, damit Welten und Inventare erhalten bleiben. Öffentliche Server
        mit aktivierter Echtheitsprüfung lehnen solche Konten ab.
      </p>
      <form className="acc__offline" onSubmit={addOffline}>
        <input className="input" placeholder="Spielername"
               value={offlineName} maxLength={16}
               onChange={(e) => setOfflineName(e.target.value.replace(/[^\w]/g, ""))}
               aria-label="Spielername für Offline-Konto" />
        <Button tone="outline" icon="plus" type="submit"
                busy={busy} disabled={!offlineName.trim()}>
          Anlegen
        </Button>
      </form>

      {device && (
        <DeviceCode
          start={device}
          onDone={async (konto) => {
            setDevice(null);
            await reload();
            notify({ kind: "ok", title: `Angemeldet als ${konto.username}` });
          }}
          onCancel={() => setDevice(null)}
        />
      )}
    </section>
  );
}

/* ---- Microsoft-Anmeldung --------------------------------------------- */

interface DeviceLogin {
  user_code: string; verification_uri: string; device_code: string; interval: number;
}

/** Zeigt den Code und fragt so lange nach, bis die Anmeldung steht.
 *
 *  Der Knopf allein half nicht: er holte den Code und warf ihn weg. Ohne
 *  Anzeige und ohne Nachfragen kam nie ein Konto zustande.
 *
 *  Der Abstand zwischen zwei Abfragen kommt von Microsoft (`interval`,
 *  meist 5s) und wird bei einem `slow_down` vergroessert -- ein fester,
 *  zu kurzer Abstand kann dazu fuehren, dass Microsoft die Anmeldung
 *  zwischendurch drosselt oder ganz ablehnt. */
function DeviceCode({ start, onDone, onCancel }: {
  start: DeviceLogin;
  onDone: (a: Account) => void | Promise<void>;
  onCancel: () => void;
}) {
  const [fehler, setFehler] = useState("");
  const [kopiert, setKopiert] = useState(false);

  useEffect(() => {
    let laeuft = true;
    let abstandMs = Math.max(start.interval, 1) * 1000;
    const tick = async () => {
      while (laeuft) {
        await new Promise((r) => setTimeout(r, abstandMs));
        if (!laeuft) return;
        try {
          const ergebnis = await api.auth.poll(start.device_code);
          if (ergebnis.status === "ready") { laeuft = false; void onDone(ergebnis.account); return; }
          if (ergebnis.status === "slow_down") { abstandMs += 5000; }
        } catch (e) {
          laeuft = false;
          setFehler(String(e));
          return;
        }
      }
    };
    void tick();
    return () => { laeuft = false; };
  }, [start.device_code, start.interval, onDone]);

  return (
    <div className="sheet__scrim" onClick={onCancel}>
      <div className="dev" role="dialog" aria-label="Microsoft-Anmeldung"
           onClick={(e) => e.stopPropagation()}>
        <h2 className="set__h">Bei Microsoft anmelden</h2>
        <p className="set__note">
          Öffne die Seite und gib den Code ein. Sobald das erledigt ist,
          erscheint das Konto hier von selbst.
        </p>

        <button className="dev__code" title="Code kopieren"
                onClick={() => {
                  void navigator.clipboard.writeText(start.user_code);
                  setKopiert(true);
                }}>
          {start.user_code}
          <Icon name={kopiert ? "check" : "clone"} size={16} />
        </button>

        <Button tone="primary" icon="compass"
                onClick={() => void api.system.openExternal(start.verification_uri)}>
          {start.verification_uri}
        </Button>

        {fehler
          ? <p className="dev__err">{fehler}</p>
          : <p className="dev__wait"><span className="btn__spin" /> Warte auf die Anmeldung…</p>}

        <Button tone="ghost" onClick={onCancel}>Abbrechen</Button>
      </div>
    </div>
  );
}

/* ---- Spiel ----------------------------------------------------------- */

function Game() {
  const config = useApp((s) => s.config)!;
  const patch = useApp((s) => s.patchConfig);

  return (
    <section className="pane pane--form">
      <h2 className="set__h">Arbeitsspeicher</h2>
      <div className="field__row">
        <Field label="Minimum (MB)">
          <input className="input" type="number" step={512} value={config.min_ram}
                 onChange={(e) => void patch({ min_ram: +e.target.value })} />
        </Field>
        <Field label="Maximum (MB)" hint="Mehr als die Hälfte des Systemspeichers bringt selten etwas.">
          <input className="input" type="number" step={512} value={config.max_ram}
                 onChange={(e) => void patch({ max_ram: +e.target.value })} />
        </Field>
      </div>

      <h2 className="set__h">Java</h2>
      <Field label="Pfad" hint="Leer lassen für die automatische Erkennung.">
        <input className="input" value={config.java_path} placeholder="automatisch"
               onChange={(e) => void patch({ java_path: e.target.value })} />
      </Field>

      <Row label="ZGC verwenden"
           hint="Moderner Garbage Collector mit sehr kurzen Pausen. Braucht Java 17 oder neuer."
           on={config.use_zgc} set={(v) => void patch({ use_zgc: v })} />

      <h2 className="set__h">Start</h2>
      <Row label="Launcher beim Start schließen" on={config.close_on_launch}
           set={(v) => void patch({ close_on_launch: v })} />
      <Row label="Hohe Prozesspriorität" on={config.high_process_priority}
           set={(v) => void patch({ high_process_priority: v })} />
      <Row label="Discord-Anwesenheit anzeigen" on={config.discord_rpc}
           set={(v) => void patch({ discord_rpc: v })} />
    </section>
  );
}

/* ---- Darstellung ----------------------------------------------------- */

const ACCENTS = ["#7c5cff", "#3b82f6", "#10b981", "#f59e0b", "#ef4444", "#ec4899", "#14b8a6"];

function Look() {
  const config = useApp((s) => s.config)!;
  const patch = useApp((s) => s.patchConfig);

  return (
    <section className="pane pane--form">
      <h2 className="set__h">Erscheinungsbild</h2>
      <Segmented
        label="Helligkeit" value={config.theme}
        onChange={(v) => void patch({ theme: v })}
        options={[{ value: "dark", label: "Dunkel" }, { value: "light", label: "Hell" }]}
      />

      <h2 className="set__h">Akzentfarbe</h2>
      <p className="set__note">
        Die Akzentfarbe bestimmt nicht nur Knöpfe — auch Grundflächen, Schein und
        Kachelfarben leiten sich daraus ab.
      </p>
      <div className="swatches">
        {ACCENTS.map((hex) => (
          <button key={hex}
                  className={`swatch${config.accent_color === hex ? " is-on" : ""}`}
                  style={{ background: hex }}
                  aria-label={`Akzentfarbe ${hex}`}
                  onClick={() => void patch({ accent_color: hex })} />
        ))}
        <input type="color" className="swatch swatch--pick" value={config.accent_color}
               onChange={(e) => void patch({ accent_color: e.target.value })}
               aria-label="Eigene Akzentfarbe" />
      </div>

      <Field label={`Grundhelligkeit: ${config.background_shade}`}
             hint="Hebt den Hintergrund an, wenn der dunkle Grund zu tief wirkt.">
        <input type="range" min={0} max={100} value={config.background_shade}
               onChange={(e) => void patch({ background_shade: +e.target.value })} />
      </Field>

      <Row label="Sparsame Darstellung"
           hint="Schaltet Weichzeichner und Bewegung ab. Hilft auf schwacher Grafik."
           on={config.performance_mode} set={(v) => void patch({ performance_mode: v })} />
    </section>
  );
}

/* ---- Erweitert ------------------------------------------------------- */

function Advanced() {
  const config = useApp((s) => s.config)!;
  const patch = useApp((s) => s.patchConfig);
  const notify = useApp((s) => s.notify);
  const [cfStatus, setCfStatus] = useState<api.CurseforgeStatus | null>(null);
  const [cfDraft, setCfDraft] = useState("");
  const [cfBusy, setCfBusy] = useState(false);

  useEffect(() => {
    void api.curseforge.status().then(setCfStatus).catch(() => setCfStatus(null));
  }, []);

  async function saveCurseforgeKey() {
    setCfBusy(true);
    try {
      const next = await api.curseforge.setKey(cfDraft.trim() || null);
      setCfStatus(next);
      setCfDraft("");
      notify(next.configured
        ? { kind: "ok", title: "CurseForge-Schlüssel hinterlegt" }
        : { kind: "info", title: "CurseForge-Schlüssel entfernt" });
    } catch (e) {
      notify({ kind: "risk", title: "Schlüssel abgelehnt", detail: String(e) });
    } finally { setCfBusy(false); }
  }

  return (
    <section className="pane pane--form">
      <h2 className="set__h">Downloads</h2>
      <Field label={`Gleichzeitige Downloads: ${config.concurrent_downloads}`}>
        <input type="range" min={1} max={32} value={config.concurrent_downloads}
               onChange={(e) => void patch({ concurrent_downloads: +e.target.value })} />
      </Field>
      <Row label="Prüfsummen verifizieren"
           hint="Vergleicht jede geladene Datei mit ihrer SHA1-Summe."
           on={config.verify_sha1} set={(v) => void patch({ verify_sha1: v })} />

      <h2 className="set__h">Mods</h2>
      <Row label="CurseForge einbeziehen" on={config.curseforge_enabled ?? true}
           set={(v) => void patch({ curseforge_enabled: v })} />
      {(config.curseforge_enabled ?? true) && (
        <div className="cf__key">
          {cfStatus?.configured ? (
            <div className="acc is-active">
              <Icon name="check" size={17} />
              <div className="mod__id">
                <span className="mod__name">CurseForge-Schlüssel hinterlegt</span>
                <span className="mod__file">{cfStatus.key_hint}</span>
              </div>
              {cfStatus.from_env && <Badge tone="warn">aus Umgebung</Badge>}
              <Button size="sm" tone="danger" icon="trash"
                      onClick={() => { setCfDraft(""); void saveCurseforgeKey(); }}>
                Entfernen
              </Button>
            </div>
          ) : (
            <p className="set__note">
              Ohne Schlüssel läuft die Suche allein über Modrinth weiter — CurseForge
              liefert dann einfach keine zusätzlichen Treffer. Einen kostenlosen
              Schlüssel gibt es unter{" "}
              <a href="#" onClick={(e) => {
                e.preventDefault();
                void api.system.openExternal("https://console.curseforge.com/");
              }}>console.curseforge.com</a>.
            </p>
          )}
          <Field label="CurseForge-API-Schlüssel">
            <div className="cf__key-row">
              <input className="input" type="password" autoComplete="off"
                     placeholder={cfStatus?.configured ? "Neuen Schlüssel eintragen…" : "Schlüssel einfügen"}
                     value={cfDraft} onChange={(e) => setCfDraft(e.target.value)} />
              <Button tone="primary" icon="check" busy={cfBusy}
                      disabled={!cfDraft.trim()} onClick={() => void saveCurseforgeKey()}>
                Speichern
              </Button>
            </div>
          </Field>
        </div>
      )}
      <Row label="Auf Mod-Updates prüfen" on={config.mod_update_check ?? true}
           set={(v) => void patch({ mod_update_check: v })} />

      <h2 className="set__h">Sicherungen</h2>
      <Row label="Vor Mod-Änderungen automatisch sichern"
           on={config.auto_snapshot ?? true} set={(v) => void patch({ auto_snapshot: v })} />

      <h2 className="set__h">Launcher</h2>
      <Row label="Automatisch aktualisieren" on={config.auto_update}
           set={(v) => void patch({ auto_update: v })} />
    </section>
  );
}

/* ---- Zeile mit Schalter ---------------------------------------------- */

function Row({ label, hint, on, set }: {
  label: string; hint?: string; on: boolean; set: (v: boolean) => void;
}) {
  return (
    <div className="setrow">
      <div className="setrow__text">
        <span className="setrow__label">{label}</span>
        {hint && <span className="field__hint">{hint}</span>}
      </div>
      <Switch checked={on} onChange={set} label={label} />
    </div>
  );
}

/* ---- Assistent --------------------------------------------------------
   Optional. Ohne Schlüssel bleibt der Launcher vollständig bedienbar:
   die Absturzanalyse läuft dann über die Regeltafel, die Suche über
   Stichwörter.                                                        */

function Assistant() {
  const notify = useApp((s) => s.notify);
  const config = useApp((s) => s.config);
  const patch = useApp((s) => s.patchConfig);
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [models, setModels] = useState<AiModel[]>([]);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);

  const load = () => void api.ai.status().then(setStatus).catch(() => setStatus(null));
  useEffect(load, []);

  /* Die Modellliste kommt vom Dienst, nicht aus einer fest verdrahteten
     Aufzählung -- so steht dort genau das, was der Schlüssel freischaltet.
     Zurückgezogene Modelle tauchen dadurch gar nicht erst auf, auch wenn
     die Doku sie noch führt. */
  useEffect(() => {
    if (!status?.configured) { setModels([]); return; }
    void api.ai.models().then(setModels).catch(() => setModels([]));
  }, [status?.configured]);

  /* Steht in der Konfiguration ein Modell, das es nicht mehr gibt, würde
     der Launcher es stur weiterbenutzen und scheitern. */
  const verschwunden =
    models.length > 0 && !!status && !models.some((m) => m.id === status.model);

  async function save() {
    setBusy(true);
    try {
      const next = await api.ai.setKey(draft.trim() || null);
      setStatus(next);
      setDraft("");
      notify(next.configured
        ? { kind: "ok", title: "Schlüssel hinterlegt" }
        : { kind: "info", title: "Schlüssel entfernt" });
    } catch (e) {
      notify({ kind: "risk", title: "Schlüssel abgelehnt", detail: String(e) });
    } finally { setBusy(false); }
  }

  return (
    <section className="pane pane--form">
      <h2 className="set__h">Mistral-Zugang</h2>
      <p className="set__note">
        Der Assistent ist freiwillig. Ohne Schlüssel arbeitet der Launcher
        vollständig: die Absturzanalyse über die eingebaute Regeltafel, die
        Suche über Stichwörter. Mit Schlüssel kommen die Deutung freier
        Suchwünsche, Konflikterklärungen im Klartext und eine zweite Meinung
        zur Absturzursache dazu.
      </p>

      {status?.configured ? (
        <div className="acc is-active">
          <Icon name="check" size={17} />
          <div className="mod__id">
            <span className="mod__name">Schlüssel hinterlegt</span>
            <span className="mod__file">{status.key_hint}</span>
          </div>
          {status.from_env && <Badge tone="warn">aus Umgebung</Badge>}
          <Button size="sm" tone="danger" icon="trash"
                  onClick={() => { setDraft(""); void save(); }}>
            Entfernen
          </Button>
        </div>
      ) : (
        <Empty icon="spark" title="Kein Schlüssel hinterlegt"
               detail="Einen Schlüssel erstellst du im Mistral-Konto unter „API Keys“." />
      )}

      <Field label="API-Schlüssel"
             hint="Wird in secrets.json im Datenverzeichnis abgelegt, nur für dich lesbar, und nie ins Repo übernommen. Eine gesetzte Umgebungsvariable MISTRAL_API_KEY hat Vorrang.">
        <input className="input" type="password" autoComplete="off"
               placeholder={status?.configured ? "Neuen Schlüssel eintragen…" : "Schlüssel einfügen"}
               value={draft} onChange={(e) => setDraft(e.target.value)} />
      </Field>

      <div className="acc__add">
        <Button tone="primary" icon="check" busy={busy}
                disabled={!draft.trim()} onClick={() => void save()}>
          Speichern
        </Button>
      </div>

      {status?.configured && (
        <>
          <h2 className="set__h">Modell</h2>
          <p className="set__note">
            Kleinere Modelle antworten schneller und haben beim kostenlosen
            Zugang die großzügigere Ratengrenze. Die Liste kommt von Mistral
            und zeigt, was dein Schlüssel freischaltet.
          </p>
          {verschwunden && (
            <p className="set__warn">
              <Icon name="warn" size={14} />
              <span>
                <strong>{status.model}</strong> steht in deinem Zugang nicht mehr zur
                Verfügung — es wurde vermutlich zurückgezogen. Wähle ein anderes,
                sonst schlagen die Assistenzfunktionen fehl.
              </span>
            </p>
          )}

          {status.rejected.length > 0 && (
            <p className="set__note">
              Dieses Modell nimmt {status.rejected.join(" und ")} nicht an —
              der Launcher lässt es deshalb weg. Die zugehörigen Einstellungen
              bleiben dadurch wirkungslos.
            </p>
          )}

          <Field label="Verwendetes Modell">
            <select className="input" value={status.model}
                    onChange={(e) => {
                      const model = e.target.value;
                      setStatus({ ...status, model });
                      void api.ai.setModel(model);
                    }}>
              {models.length === 0 && <option value={status.model}>{status.model}</option>}
              {models.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.id}{m.description ? ` — ${m.description}` : ""}
                </option>
              ))}
            </select>
          </Field>

          <h2 className="set__h">Verhalten</h2>
          <Row label="Denkaufwand anfordern"
               hint="Das Modell denkt gründlicher, die Antwort dauert länger. Nimmt ein Modell den Parameter nicht an, lässt der Launcher ihn von allein weg."
               on={config?.ai_reasoning ?? true}
               set={(v) => void patch({ ai_reasoning: v })} />
          <Row label="Werkzeuge bei der Absturzanalyse"
               hint="Websuche und Codeausführung — nur bei Abstürzen, die keine der eingebauten Regeln erkennt."
               on={config?.ai_tools ?? true}
               set={(v) => void patch({ ai_tools: v })} />
        </>
      )}
    </section>
  );
}
