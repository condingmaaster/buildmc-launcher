/* =========================================================================
   Bedrock Edition -- eigene, schlanke Instanzverwaltung.

   Bewusst kein Kontenbereich: welches Google-Konto bei mcpelauncher-ui-qt
   verknuepft ist, laesst sich nicht auslesen (verschluesselt, siehe Plan).
   Was BuildMC zeigt: ob mcpelauncher gefunden wurde, ob laut einfacher
   Existenzpruefung ueberhaupt ein Konto verknuepft ist, und die bereits
   heruntergeladenen Versionen als Grundlage fuer eigene Instanzen.
   ========================================================================= */

import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Badge, Button, Empty } from "../components/ui";
import { ago, playtime } from "../lib/format";
import type { BedrockStatus, BedrockVersion } from "../lib/types";
import "./bedrock.css";

export function Bedrock() {
  const notify = useApp((s) => s.notify);
  const instances = useApp((s) => s.bedrockInstances);
  const running = useApp((s) => s.bedrockRunning);
  const reloadInstances = useApp((s) => s.reloadBedrockInstances);
  const startInstance = useApp((s) => s.startBedrockInstance);
  const stopInstance = useApp((s) => s.stopBedrockInstance);

  const [status, setStatus] = useState<BedrockStatus | null>(null);
  const [versions, setVersions] = useState<BedrockVersion[]>([]);
  const [busy, setBusy] = useState(false);
  const [newName, setNewName] = useState("");
  const [newVersion, setNewVersion] = useState("");

  const loadStatus = () => void api.bedrock.status().then(setStatus).catch(() => setStatus(null));
  const loadVersions = () => void api.bedrock.listVersions().then((v) => {
    setVersions(v);
    if (!newVersion && v[0]) setNewVersion(v[0].dir_name);
  });

  useEffect(() => {
    loadStatus();
    loadVersions();
    void reloadInstances();
  }, [reloadInstances]);

  async function openUi() {
    setBusy(true);
    try {
      await api.bedrock.openUi();
      notify({ kind: "info", title: "mcpelauncher-ui-qt geöffnet", detail: "Nach dem Anmelden/Herunterladen hier auf „Versionen neu einlesen“ drücken." });
    } catch (e) {
      notify({ kind: "risk", title: "Ließ sich nicht öffnen", detail: String(e) });
    } finally { setBusy(false); }
  }

  function refresh() {
    loadStatus();
    loadVersions();
  }

  async function createInstance(e: React.FormEvent) {
    e.preventDefault();
    if (!newName.trim() || !newVersion) return;
    setBusy(true);
    try {
      await api.bedrock.createInstance(newName.trim(), newVersion);
      setNewName("");
      await reloadInstances();
      notify({ kind: "ok", title: `Instanz „${newName.trim()}“ angelegt` });
    } catch (e) {
      notify({ kind: "risk", title: "Anlegen fehlgeschlagen", detail: String(e) });
    } finally { setBusy(false); }
  }

  if (!status) return null;

  return (
    <>
      <header className="topbar">
        <div className="topbar__lead"><h1 className="topbar__title">Bedrock</h1></div>
      </header>

      <div className="bed u-scroll">
        <section className="bed__card">
          <h2 className="bed__h">Status</h2>
          <div className="bed__status-row">
            <Badge tone={status.client_found ? "ok" : "neutral"}>
              {status.client_found ? "mcpelauncher gefunden" : "mcpelauncher nicht gefunden"}
            </Badge>
            <Badge tone={status.google_account_linked ? "ok" : "neutral"}>
              {status.google_account_linked ? "Konto verknüpft" : "Kein Konto verknüpft"}
            </Badge>
            <Badge tone="neutral">{status.available_versions} Version(en) heruntergeladen</Badge>
          </div>

          {!status.client_found && (
            <p className="bed__note">
              Läuft nur unter Linux, mit installiertem{" "}
              <a href="#" onClick={(e) => { e.preventDefault(); void api.system.openExternal("https://minecraft-linux.github.io/"); }}>
                mcpelauncher
              </a>. Nicht Teil dieses Launchers — externes, quelloffenes Programm.
            </p>
          )}

          <div className="bed__actions">
            <Button tone="primary" icon="download" busy={busy} disabled={!status.ui_found}
                    onClick={() => void openUi()}>
              Google-Konto verknüpfen / Version herunterladen
            </Button>
            <Button tone="ghost" icon="refresh" onClick={refresh}>Versionen neu einlesen</Button>
          </div>
        </section>

        <section className="bed__card">
          <h2 className="bed__h">Instanz anlegen</h2>
          {versions.length === 0 ? (
            <Empty icon="bedrock" title="Noch keine Version heruntergeladen"
                   detail="Zuerst oben ein Konto verknüpfen und eine Version herunterladen." />
          ) : (
            <form className="bed__create" onSubmit={(e) => void createInstance(e)}>
              <input className="input" placeholder="Name der Instanz" value={newName}
                     onChange={(e) => setNewName(e.target.value)} maxLength={32} />
              <select className="input" value={newVersion} onChange={(e) => setNewVersion(e.target.value)}>
                {versions.map((v) => <option key={v.dir_name} value={v.dir_name}>{v.dir_name}</option>)}
              </select>
              <Button tone="outline" icon="plus" type="submit" busy={busy} disabled={!newName.trim()}>
                Anlegen
              </Button>
            </form>
          )}
        </section>

        <section className="bed__card">
          <h2 className="bed__h">Instanzen</h2>
          {instances.length === 0 ? (
            <Empty icon="bedrock" title="Keine Instanz" detail="Oben eine anlegen, um loszuspielen." />
          ) : (
            <ul className="bed__list">
              {instances.map((inst) => {
                const isRunning = running.some((r) => r.instance_id === inst.id);
                return (
                  <li className={`bed__item${isRunning ? " is-running" : ""}`} key={inst.id}>
                    <Icon name="bedrock" size={22} />
                    <div className="mod__id">
                      <span className="mod__name">{inst.name}</span>
                      <span className="mod__file">
                        {inst.version_dir_name} · {playtime(inst.play_seconds)} ·{" "}
                        {inst.last_played ? `zuletzt ${ago(inst.last_played)}` : "noch nie gespielt"}
                      </span>
                    </div>
                    {isRunning && <Badge tone="ok"><span className="live" />Läuft</Badge>}
                    <Button tone={isRunning ? "outline" : "primary"} icon={isRunning ? "stop" : "play"}
                            onClick={() => void (isRunning ? stopInstance(inst.id) : startInstance(inst.id))}>
                      {isRunning ? "Beenden" : "Spielen"}
                    </Button>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      </div>
    </>
  );
}
