import { useEffect, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Badge, Button, Empty } from "../components/ui";
import { num } from "../lib/format";
import type { LanServer, SavedServer } from "../lib/types";
import "./servers.css";

export function Servers() {
  const notify = useApp((s) => s.notify);
  const instances = useApp((s) => s.instances);
  const startInstance = useApp((s) => s.startInstance);
  const [list, setList] = useState<SavedServer[] | null>(null);
  const [lan, setLan] = useState<LanServer[]>([]);
  const [scanning, setScanning] = useState(false);
  const [name, setName] = useState("");
  const [address, setAddress] = useState("");
  /* Beitreten heisst: *eine* Instanz startet und verbindet. Welche, muss
     hier gewaehlt werden -- der Server allein sagt es nicht. */
  const [target, setTarget] = useState("");
  const [joining, setJoining] = useState("");

  // Zuletzt gespielte Instanz als Vorgabe.
  useEffect(() => {
    if (target || !instances.length) return;
    const zuletzt = [...instances].sort(
      (a, b) => (b.last_played ?? "").localeCompare(a.last_played ?? ""),
    )[0];
    if (zuletzt) setTarget(zuletzt.id);
  }, [instances, target]);

  async function join(addr: string) {
    if (!target) {
      notify({ kind: "warn", title: "Keine Instanz gewählt",
               detail: "Oben rechts festlegen, womit beigetreten werden soll." });
      return;
    }
    setJoining(addr);
    try {
      await startInstance(target, { kind: "server", address: addr });
    } catch (e) {
      notify({ kind: "risk", title: "Beitritt fehlgeschlagen", detail: String(e) });
    } finally {
      setJoining("");
    }
  }

  const load = () => void api.servers.list().then(setList).catch(() => setList([]));
  useEffect(load, []);

  async function scan() {
    setScanning(true);
    try { setLan(await api.lan.discover()); }
    finally { setScanning(false); }
  }

  async function add(e: React.FormEvent) {
    e.preventDefault();
    if (!address.trim()) return;
    await api.servers.save(name.trim() || address.trim(), address.trim());
    setName(""); setAddress("");
    notify({ kind: "ok", title: "Server gespeichert" });
    load();
  }

  return (
    <>
      <header className="topbar">
        <div className="topbar__lead">
          <h1 className="topbar__title">Server</h1>
          <span className="topbar__count">{num(list?.length ?? 0)} gespeichert</span>
        </div>
        <div className="topbar__spacer" />
        <label className="srv__target">
          <span>Beitreten mit</span>
          <select className="input" value={target} disabled={!instances.length}
                  onChange={(e) => setTarget(e.target.value)}>
            {!instances.length && <option value="">Keine Instanz vorhanden</option>}
            {instances.map((i) => (
              <option key={i.id} value={i.id}>{i.name}</option>
            ))}
          </select>
        </label>
        <Button icon="refresh" busy={scanning} onClick={() => void scan()}>
          LAN durchsuchen
        </Button>
      </header>

      <div className="srv u-scroll">
        <form className="srv__add" onSubmit={add}>
          <input className="input" placeholder="Name (optional)"
                 value={name} onChange={(e) => setName(e.target.value)}
                 aria-label="Servername" />
          <input className="input" placeholder="Adresse, z. B. play.buildmc.io"
                 value={address} onChange={(e) => setAddress(e.target.value)}
                 aria-label="Serveradresse" />
          <Button tone="primary" icon="plus" type="submit" disabled={!address.trim()}>
            Hinzufügen
          </Button>
        </form>

        {list && !list.length && (
          <Empty icon="server" title="Keine Server gespeichert"
                 detail="Trag oben eine Adresse ein oder durchsuche das lokale Netz." />
        )}

        <ul className="srv__list u-stagger">
          {list?.map((s) => (
            <li className="srv__item" key={s.id}>
              <span className={`srv__pulse${s.status?.online ? " is-on" : ""}`} />

              <div className="srv__id">
                <span className="srv__name u-truncate">{s.name}</span>
                <span className="srv__addr u-truncate">{s.address}</span>
              </div>

              {s.status?.online ? (
                <>
                  <span className="srv__motd u-truncate">{s.status.motd}</span>
                  <Badge tone="ok">{s.status.players_online}/{s.status.players_max}</Badge>
                  <Badge>{s.status.latency_ms} ms</Badge>
                </>
              ) : (
                <Badge tone="risk">offline</Badge>
              )}

              <Button size="sm" tone="primary" icon="play"
                      busy={joining === s.address} disabled={!target}
                      onClick={() => void join(s.address)}>
                Beitreten
              </Button>

              <button className="mod__x" aria-label={`${s.name} entfernen`}
                      onClick={() => void api.servers.remove(s.id).then(load)}>
                <Icon name="trash" size={15} />
              </button>
            </li>
          ))}
        </ul>

        {lan.length > 0 && (
          <section className="srv__lan">
            <h2 className="strip__title">Im lokalen Netz</h2>
            <ul className="srv__list">
              {lan.map((l) => (
                <li className="srv__item" key={`${l.address}:${l.port}`}>
                  <span className="srv__pulse is-on" />
                  <div className="srv__id">
                    <span className="srv__name u-truncate">{l.motd}</span>
                    <span className="srv__addr">{l.address}:{l.port}</span>
                  </div>
                  <Button size="sm" tone="primary" icon="play"
                          busy={joining === `${l.address}:${l.port}`} disabled={!target}
                          onClick={() => void join(`${l.address}:${l.port}`)}>
                    Beitreten
                  </Button>
                </li>
              ))}
            </ul>
          </section>
        )}
      </div>
    </>
  );
}
