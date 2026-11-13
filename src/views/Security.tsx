/* =========================================================================
   Sicherheitscenter.

   Pflichtpruefung vor jedem Start (Backend: launch::start ruft
   security::verify_instance auf) -- diese Ansicht ist die Verwaltung
   dazu: Status, manuelles Nachpruefen, Verlauf, Fehlalarme uebergehen.

   Bewusst keine Behauptung von Echtzeitschutz: VirusTotal ist eine
   passive Zweitmeinung per Hash-Abfrage, ClamAV ein lokaler Scanner als
   externer Prozess. Beides zusammen deckt ab, was der Launcher jemals
   ausfuehrt -- kein Ersatz fuer echte Antiviren-Software auf dem System.
   ========================================================================= */

import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Badge, Button, Empty, Segmented, Switch } from "../components/ui";
import { ago } from "../lib/format";
import type { FileVerdict, SecurityMode, SecurityStatus } from "../lib/api";
import "./security.css";

export function Security() {
  const notify = useApp((s) => s.notify);
  const instances = useApp((s) => s.instances);

  const [status, setStatus] = useState<SecurityStatus | null>(null);
  const [draft, setDraft] = useState("");
  const [busy, setBusy] = useState(false);
  const [history, setHistory] = useState<FileVerdict[]>([]);

  const [targetId, setTargetId] = useState("");
  const [scanning, setScanning] = useState(false);
  const [scanResult, setScanResult] = useState<FileVerdict[] | null>(null);
  const [kopiert, setKopiert] = useState(false);

  const INSTALL_BEFEHL = "sudo apt install clamav clamav-daemon";

  const loadStatus = () => void api.security.status().then(setStatus).catch(() => setStatus(null));
  const loadHistory = () => void api.security.history().then(setHistory).catch(() => setHistory([]));

  useEffect(() => {
    loadStatus();
    loadHistory();
  }, []);

  useEffect(() => {
    if (!targetId && instances.length) setTargetId(instances[0]!.id);
  }, [instances, targetId]);

  async function saveKey() {
    setBusy(true);
    try {
      const next = await api.security.setKey(draft.trim() || null);
      setStatus(next);
      setDraft("");
      notify(next.virustotal_configured
        ? { kind: "ok", title: "VirusTotal-Schlüssel hinterlegt" }
        : { kind: "info", title: "Schlüssel entfernt" });
    } catch (e) {
      notify({ kind: "risk", title: "Schlüssel abgelehnt", detail: String(e) });
    } finally { setBusy(false); }
  }

  async function setMode(mode: SecurityMode) {
    await api.security.setMode(mode);
    loadStatus();
  }

  async function setAutoScan(v: boolean) {
    await api.security.setAutoScan(v);
    loadStatus();
  }

  async function scanNow() {
    if (!targetId) return;
    setScanning(true);
    setScanResult(null);
    try {
      const funde = await api.security.scanInstance(targetId);
      setScanResult(funde);
      loadHistory();
      notify(funde.length
        ? { kind: "warn", title: `${funde.length} auffällige Datei(en)` }
        : { kind: "ok", title: "Nichts Auffälliges gefunden" });
    } catch (e) {
      notify({ kind: "risk", title: "Prüfung fehlgeschlagen", detail: String(e) });
    } finally { setScanning(false); }
  }

  async function allow(sha1: string) {
    await api.security.allowHash(sha1);
    notify({ kind: "info", title: "Als Ausnahme vermerkt" });
    loadHistory();
    setScanResult((prev) => prev?.map((f) => f.sha1 === sha1 ? { ...f, allowed: true } : f) ?? null);
  }

  async function uploadUnknown(filename: string) {
    if (!targetId) return;
    try {
      await api.security.uploadUnknown(targetId, filename);
      notify({ kind: "ok", title: `${filename} zur Analyse hochgeladen` });
    } catch (e) {
      notify({ kind: "risk", title: "Hochladen fehlgeschlagen", detail: String(e) });
    }
  }

  if (!status) return null;

  return (
    <>
      <header className="topbar">
        <div className="topbar__lead">
          <h1 className="topbar__title">Sicherheit</h1>
          <Badge tone={status.mode === "block" ? "risk" : "accent"}>
            {status.mode === "block" ? "Blockieren" : "Warnen"}
          </Badge>
        </div>
      </header>

      <div className="sec u-scroll">
        <section className="sec__card">
          <h2 className="sec__h">VirusTotal</h2>
          <p className="sec__note">
            Passive Zweitmeinung per Hash-Abfrage gegen rund 70 Antiviren-Engines. Kein
            eigener Wächter — nur eine Frage: „wurde genau diese Datei schon einmal
            irgendwo als bösartig gemeldet?". Braucht einen persönlichen, kostenlosen
            Schlüssel.
          </p>
          {status.virustotal_configured ? (
            <div className="acc is-active">
              <Icon name="check" size={17} />
              <div className="mod__id">
                <span className="mod__name">Schlüssel hinterlegt</span>
                <span className="mod__file">{status.virustotal_key_hint}</span>
              </div>
              <Button size="sm" tone="danger" icon="trash"
                      onClick={() => { setDraft(""); void saveKey(); }}>
                Entfernen
              </Button>
            </div>
          ) : (
            <Empty icon="shield" title="Kein Schlüssel hinterlegt"
                   detail="Kostenlos erstellbar unter virustotal.com/gui/my-apikey." />
          )}
          <div className="sec__key-row">
            <input className="input" type="password" autoComplete="off"
                   placeholder={status.virustotal_configured ? "Neuen Schlüssel eintragen…" : "Schlüssel einfügen"}
                   value={draft} onChange={(e) => setDraft(e.target.value)} />
            <Button tone="primary" icon="check" busy={busy}
                    disabled={!draft.trim()} onClick={() => void saveKey()}>
              Speichern
            </Button>
          </div>
        </section>

        <section className="sec__card">
          <h2 className="sec__h">ClamAV</h2>
          <p className="sec__note">
            Lokaler Scanner, als eigenständiger Prozess aufgerufen — läuft ohne Internet.
            {status.clamav_available
              ? " clamdscan wird bevorzugt, sofern der Dienst läuft (schneller als clamscan)."
              : " Nicht gefunden — diese Schicht wird übersprungen, kein Fehler."}
          </p>
          <Badge tone={status.clamav_available ? "ok" : "neutral"}>
            {status.clamav_available ? "Gefunden" : "Nicht installiert"}
          </Badge>
          {!status.clamav_available && (
            <div className="sec__install">
              <span className="field__hint">
                Der Launcher installiert das nicht automatisch — von Hand nachrüsten:
              </span>
              <button className="sec__cmd" title="Befehl kopieren"
                      onClick={() => {
                        void navigator.clipboard.writeText(INSTALL_BEFEHL);
                        setKopiert(true);
                        setTimeout(() => setKopiert(false), 2000);
                      }}>
                <code>{INSTALL_BEFEHL}</code>
                <Icon name={kopiert ? "check" : "clone"} size={14} />
              </button>
              <Button size="sm" tone="ghost" onClick={loadStatus}>Erneut prüfen</Button>
            </div>
          )}
        </section>

        <section className="sec__card">
          <h2 className="sec__h">Verhalten</h2>
          <div className="setrow">
            <div className="setrow__text">
              <span className="setrow__label">Pflichtprüfung vor jedem Start</span>
              <span className="field__hint">Prüft jede .jar im Mods-Ordner, nicht nur frische Downloads.</span>
            </div>
            <Switch checked={status.auto_scan} onChange={setAutoScan}
                    label="Pflichtprüfung vor jedem Start" />
          </div>
          <div className="sec__mode">
            <span className="field__label">Bei einem Fund</span>
            <Segmented value={status.mode} onChange={(v) => void setMode(v)}
                       label="Bei einem Fund"
                       options={[
                         { value: "warn", label: "Warnen" },
                         { value: "block", label: "Blockieren" },
                       ]} />
          </div>
        </section>

        <section className="sec__card">
          <h2 className="sec__h">Instanz jetzt prüfen</h2>
          <div className="sec__scan-row">
            <select className="input" value={targetId} onChange={(e) => setTargetId(e.target.value)}>
              {instances.map((i) => <option key={i.id} value={i.id}>{i.name}</option>)}
            </select>
            <Button tone="primary" icon="shield" busy={scanning}
                    disabled={!targetId} onClick={() => void scanNow()}>
              Jetzt prüfen
            </Button>
          </div>

          {scanResult && (
            scanResult.length === 0 ? (
              <p className="sec__ok"><Icon name="check" size={14} />Nichts Auffälliges gefunden.</p>
            ) : (
              <ul className="sec__hits">
                {scanResult.map((f) => (
                  <li key={f.sha1} className={f.allowed ? "is-allowed" : ""}>
                    <Icon name="warn" size={15} />
                    <div className="mod__id">
                      <span className="mod__name">{f.filename}</span>
                      <span className="mod__file">{f.reason}</span>
                    </div>
                    {f.allowed ? (
                      <Badge tone="neutral">Übergangen</Badge>
                    ) : (
                      <Button size="sm" onClick={() => void allow(f.sha1)}>Trotzdem verwenden</Button>
                    )}
                  </li>
                ))}
              </ul>
            )
          )}
        </section>

        <section className="sec__card">
          <h2 className="sec__h">Verlauf</h2>
          {history.length === 0 ? (
            <Empty icon="shield" title="Noch keine Prüfung"
                   detail="Prüfe oben eine Instanz, um den Verlauf zu füllen." />
          ) : (
            <ul className="sec__history">
              {history.map((f) => (
                <li key={f.sha1}>
                  <Icon name={f.flagged ? "warn" : "check"} size={14} />
                  <span className="mod__name u-truncate">{f.filename}</span>
                  <span className="sec__reason u-truncate">{f.reason}</span>
                  <span className="sec__when">{ago(f.checked_at)}</span>
                  {f.flagged && !f.allowed && (
                    <>
                      <Button size="sm" onClick={() => void allow(f.sha1)}>Freigeben</Button>
                      <Button size="sm" tone="ghost" onClick={() => void uploadUnknown(f.filename)}>
                        Zur Analyse hochladen
                      </Button>
                    </>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </>
  );
}
