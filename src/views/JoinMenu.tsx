/* =========================================================================
   Beitrittsziel.

   Minecraft kann seit 1.20 per Quick Play direkt in einen Server, eine
   Welt oder ein Realm springen. Davor gab es nur --server/--port. Diese
   Unterscheidung trifft das Backend; hier wird nur gewaehlt, wohin.
   ========================================================================= */

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import * as api from "../lib/api";
import { Icon } from "../components/Icon";
import { ago } from "../lib/format";
import type { JoinTarget, LanServer, SavedServer } from "../lib/types";

interface World { folder: string; name: string; last_played: string }

export function JoinMenu({ instanceId, anchor, onPick, onClose }: {
  instanceId: string;
  /** Element, an dem das Menue ausgerichtet wird. */
  anchor: HTMLElement | null;
  onPick: (j: JoinTarget) => void;
  onClose: () => void;
}) {
  const [pos, setPos] = useState<{ top: number; left: number; flip: boolean } | null>(null);
  const [saved, setSaved] = useState<SavedServer[]>([]);
  const [worlds, setWorlds] = useState<World[]>([]);
  const [lan, setLan] = useState<LanServer[]>([]);
  const [scanning, setScanning] = useState(true);
  const [direct, setDirect] = useState("");
  const box = useRef<HTMLDivElement>(null);

  /* Das Menue haengt per Portal an <body>. Sonst schneidet es der
     Scrollbereich der Bibliothek ab, sobald es ueber dessen Rand ragt. */
  useLayoutEffect(() => {
    if (!anchor) return;
    const place = () => {
      const r = anchor.getBoundingClientRect();
      const height = box.current?.offsetHeight ?? 430;
      const below = window.innerHeight - r.bottom;
      const flip = below < height + 16 && r.top > below;
      setPos({
        top: flip ? Math.max(8, r.top - height - 8) : r.bottom + 8,
        left: Math.min(r.left, window.innerWidth - 336),
        flip,
      });
    };
    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [anchor]);

  useEffect(() => {
    let live = true;
    void (async () => {
      const [s, w] = await Promise.all([
        api.servers.list().catch(() => []),
        api.launch.worlds(instanceId).catch(() => []),
      ]);
      if (!live) return;
      setSaved(s);
      setWorlds(w);
      const found = await api.lan.discover().catch(() => []);
      if (!live) return;
      setLan(found);
      setScanning(false);
    })();
    return () => { live = false; };
  }, [instanceId]);

  /* Schliessen bei Klick nach aussen und mit Escape. */
  useEffect(() => {
    const onDown = (e: MouseEvent) => {
      if (!box.current?.contains(e.target as Node)) onClose();
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [onClose]);

  const submitDirect = (e: React.FormEvent) => {
    e.preventDefault();
    const address = direct.trim();
    if (address) onPick({ kind: "server", address });
  };

  const menu = (
    <div
      className="join u-rise"
      ref={box}
      role="menu"
      aria-label="Beitrittsziel"
      style={{
        top: pos?.top ?? -9999,
        left: pos?.left ?? -9999,
        visibility: pos ? "visible" : "hidden",
      }}
    >
      <button className="join__row join__row--lead" role="menuitem"
              onClick={() => onPick({ kind: "none" })}>
        <Icon name="play" size={15} />
        <span className="join__label">Nur starten</span>
        <span className="join__hint">Hauptmenü</span>
      </button>

      <form className="join__direct" onSubmit={submitDirect}>
        <Icon name="server" size={14} />
        <input
          className="input"
          placeholder="Adresse direkt eingeben…"
          value={direct}
          onChange={(e) => setDirect(e.target.value)}
          aria-label="Serveradresse"
        />
        <button type="submit" disabled={!direct.trim()} aria-label="Verbinden">
          <Icon name="chevron" size={14} />
        </button>
      </form>

      {saved.length > 0 && (
        <Group title="Gespeicherte Server">
          {saved.map((s) => (
            <button key={s.id} className="join__row" role="menuitem"
                    onClick={() => onPick({ kind: "server", address: s.address })}>
              <span className={`join__dot${s.status?.online ? " is-on" : ""}`} />
              <span className="join__label u-truncate">{s.name}</span>
              <span className="join__hint">
                {s.status?.online
                  ? `${s.status.players_online}/${s.status.players_max}`
                  : "offline"}
              </span>
            </button>
          ))}
        </Group>
      )}

      <Group title={scanning ? "LAN wird gesucht…" : "Im lokalen Netz"}>
        {scanning && <div className="join__scan"><span /><span /><span /></div>}
        {!scanning && !lan.length && (
          <p className="join__none">Kein offenes Spiel im Netz gefunden.</p>
        )}
        {lan.map((l) => (
          <button key={`${l.address}:${l.port}`} className="join__row" role="menuitem"
                  onClick={() => onPick({ kind: "server", address: `${l.address}:${l.port}` })}>
            <span className="join__dot is-on" />
            <span className="join__label u-truncate">{l.motd}</span>
            <span className="join__hint">{l.address}</span>
          </button>
        ))}
      </Group>

      {worlds.length > 0 && (
        <Group title="Einzelspieler">
          {worlds.map((w) => (
            <button key={w.folder} className="join__row" role="menuitem"
                    onClick={() => onPick({ kind: "world", folder: w.folder })}>
              <Icon name="folder" size={14} />
              <span className="join__label u-truncate">{w.name}</span>
              <span className="join__hint">{ago(w.last_played)}</span>
            </button>
          ))}
        </Group>
      )}
    </div>
  );

  return createPortal(menu, document.body);
}

function Group({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div className="join__group">
      <span className="join__gtitle">{title}</span>
      {children}
    </div>
  );
}
