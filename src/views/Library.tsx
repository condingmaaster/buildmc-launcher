import { useEffect, useMemo, useRef, useState } from "react";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { Badge, Button, Segmented, Empty } from "../components/ui";
import { LOADER_LABEL, ago, num, playtime } from "../lib/format";
import { CreateInstance } from "./CreateInstance";
import type { Instance, JoinTarget } from "../lib/types";
import "./library.css";

type Sort = "played" | "name" | "time";

export function Library() {
  const instances = useApp((s) => s.instances);
  const running = useApp((s) => s.running);
  const go = useApp((s) => s.go);
  const startInstance = useApp((s) => s.startInstance);
  const stopInstance = useApp((s) => s.stopInstance);

  const [focusId, setFocusId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [sort, setSort] = useState<Sort>("played");
  const [query, setQuery] = useState("");
  const [launching, setLaunching] = useState<string | null>(null);
  const [joinOpen, setJoinOpen] = useState(false);

  /* Die zuletzt gespielte Instanz steht auf der Buehne, solange der Nutzer
     nichts anderes waehlt. */
  const ordered = useMemo(() => sortBy(instances, sort, query), [instances, sort, query]);
  const focus = instances.find((i) => i.id === focusId) ?? ordered[0] ?? null;

  const groups = useMemo(() => groupBy(ordered), [ordered]);
  const isRunning = (id: string) => running.some((r) => r.instance_id === id);

  async function play(inst: Instance, join: JoinTarget = { kind: "none" }) {
    setJoinOpen(false);
    setLaunching(inst.id);
    try {
      if (isRunning(inst.id)) await stopInstance(inst.id);
      else await startInstance(inst.id, join);
    } catch {
      /* Der Store hat bereits gemeldet. */
    } finally {
      setLaunching(null);
    }
  }

  if (!instances.length) {
    return (
      <>
        <TopBar query={query} onQuery={setQuery} sort={sort} onSort={setSort}
                count={0} onCreate={() => setCreating(true)} />
        <div className="lib__empty">
          <Empty
            icon="library"
            title="Noch keine Instanz"
            detail="Eine Instanz buendelt eine Minecraft-Version, einen Loader und deine Mods. Leg die erste an, um zu starten."
            action={
              <Button tone="primary" size="lg" icon="plus"
                      onClick={() => setCreating(true)}>
                Instanz anlegen
              </Button>
            }
          />
        </div>
        {creating && <CreateInstance onClose={() => setCreating(false)} />}
      </>
    );
  }

  return (
    <>
      <TopBar query={query} onQuery={setQuery} sort={sort} onSort={setSort}
              count={ordered.length} onCreate={() => setCreating(true)} />

      <div className="lib u-scroll">
        {focus && (
          <Stage
            inst={focus}
            running={isRunning(focus.id)}
            launching={launching === focus.id}
            joinOpen={joinOpen}
            onJoinOpen={setJoinOpen}
            onPlay={play}
            onOpen={() => go({ name: "instance", id: focus.id })}
          />
        )}

        {groups.map(([name, items]) => (
          <section className="strip" key={name}>
            <header className="strip__head">
              <h2 className="strip__title">{name}</h2>
              <span className="strip__count">{num(items.length)}</span>
            </header>
            <div className="strip__rail u-stagger">
              {items.map((inst) => (
                <Tile
                  key={inst.id}
                  inst={inst}
                  focused={inst.id === focus?.id}
                  running={isRunning(inst.id)}
                  onFocus={() => setFocusId(inst.id)}
                  onOpen={() => go({ name: "instance", id: inst.id })}
                />
              ))}
            </div>
          </section>
        ))}

        {!ordered.length && (
          <Empty icon="search" title="Nichts gefunden"
                 detail={`Keine Instanz passt zu "${query}".`} />
        )}
      </div>

      {creating && <CreateInstance onClose={() => setCreating(false)} />}
    </>
  );
}

/* ---- Kopfzeile ------------------------------------------------------- */

function TopBar({ query, onQuery, sort, onSort, count, onCreate }: {
  query: string; onQuery: (v: string) => void;
  sort: Sort; onSort: (s: Sort) => void; count: number; onCreate: () => void;
}) {
  const input = useRef<HTMLInputElement>(null);

  /* Strg+F springt ins Suchfeld -- eine Tastatur-Geste, die man erwartet. */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "f") {
        e.preventDefault();
        input.current?.focus();
        input.current?.select();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  return (
    <header className="topbar">
      <div className="topbar__lead">
        <h1 className="topbar__title">Bibliothek</h1>
        <span className="topbar__count">{num(count)} Instanzen</span>
      </div>

      <div className="topbar__search">
        <Icon name="search" size={16} />
        <input
          ref={input}
          className="input"
          type="search"
          placeholder="Instanz suchen…"
          value={query}
          onChange={(e) => onQuery(e.target.value)}
          aria-label="Instanz suchen"
        />
        <kbd className="topbar__kbd">Strg F</kbd>
      </div>

      <Segmented
        label="Sortierung"
        value={sort}
        onChange={onSort}
        options={[
          { value: "played", label: "Zuletzt", icon: "clock" },
          { value: "time",   label: "Spielzeit" },
          { value: "name",   label: "Name" },
        ]}
      />

      <Button tone="primary" icon="plus" onClick={onCreate}>Neu</Button>
    </header>
  );
}

/* ---- Buehne ---------------------------------------------------------- */

function Stage({ inst, running, launching, joinOpen, onJoinOpen, onPlay, onOpen }: {
  inst: Instance; running: boolean; launching: boolean;
  joinOpen: boolean; onJoinOpen: (v: boolean) => void;
  onPlay: (i: Instance, j?: JoinTarget) => void; onOpen: () => void;
}) {
  const playBox = useRef<HTMLDivElement>(null);
  return (
    <section className={`stage${running ? " is-running" : ""}`} key={inst.id}>
      <div className="stage__body">
        <div className="stage__tags">
          <Badge tone="accent">{inst.mc_version}</Badge>
          <Badge hue={LOADER_HUE[inst.loader] ?? 200}>{LOADER_LABEL[inst.loader]}</Badge>
          {running && <Badge tone="ok"><span className="live" />Läuft</Badge>}
          {inst.mods.length > 0 && (
            <Badge><Icon name="puzzle" size={11} />{num(inst.mods.length)} Mods</Badge>
          )}
        </div>

        <h2 className="stage__name">{inst.name}</h2>

        <p className="stage__meta">
          {playtime(inst.play_seconds ?? 0)}
          <span className="dot" />
          {num(inst.launch_count ?? 0)}× gestartet
          <span className="dot" />
          zuletzt {ago(inst.last_played)}
        </p>

        <div className="stage__actions">
          <div className="stage__play" ref={playBox}>
            <Button
              tone={running ? "outline" : "primary"}
              size="lg"
              icon={running ? "stop" : "play"}
              busy={launching}
              onClick={() => onPlay(inst)}
            >
              {running ? "Beenden" : "Spielen"}
            </Button>

          </div>

          <Button tone="outline" size="lg" icon="gear" onClick={onOpen}>
            Verwalten
          </Button>
        </div>
      </div>

      <div className="stage__art" aria-hidden="true">
        <span className="stage__glyph">{inst.name.slice(0, 1).toUpperCase()}</span>
      </div>
    </section>
  );
}

/* ---- Kachel im Filmstreifen ------------------------------------------ */

function Tile({ inst, focused, running, onFocus, onOpen }: {
  inst: Instance; focused: boolean; running: boolean;
  onFocus: () => void; onOpen: () => void;
}) {
  return (
    <button
      className={`tile${focused ? " is-focused" : ""}${running ? " is-running" : ""}`}
      onClick={onFocus}
      onDoubleClick={onOpen}
      style={{ "--tile-h": String(LOADER_HUE[inst.loader] ?? 200) } as React.CSSProperties}
      aria-pressed={focused}
    >
      <span className="tile__art" aria-hidden="true">
        {inst.name.slice(0, 1).toUpperCase()}
      </span>
      <span className="tile__name u-truncate">{inst.name}</span>
      <span className="tile__sub u-truncate">
        {inst.mc_version} · {LOADER_LABEL[inst.loader]}
      </span>
      {running && <span className="tile__live" aria-label="läuft" />}
    </button>
  );
}

/* ---- Hilfsfunktionen ------------------------------------------------- */

const LOADER_HUE: Record<string, number> = {
  vanilla: 96, buildmc: 265, fabric: 36, forge: 220, neoforge: 24, quilt: 288,
};

function sortBy(list: Instance[], sort: Sort, query: string): Instance[] {
  const q = query.trim().toLowerCase();
  const filtered = q
    ? list.filter((i) =>
        i.name.toLowerCase().includes(q) ||
        i.mc_version.toLowerCase().includes(q) ||
        i.loader.toLowerCase().includes(q))
    : list;

  return [...filtered].sort((a, b) => {
    if (sort === "name") return a.name.localeCompare(b.name, "de");
    if (sort === "time") return (b.play_seconds ?? 0) - (a.play_seconds ?? 0);
    return Date.parse(b.last_played || "0") - Date.parse(a.last_played || "0");
  });
}

function groupBy(list: Instance[]): [string, Instance[]][] {
  const map = new Map<string, Instance[]>();
  for (const i of list) {
    const key = i.group ?? "Ohne Gruppe";
    (map.get(key) ?? map.set(key, []).get(key)!).push(i);
  }
  return [...map.entries()].sort((a, b) =>
    a[0] === "Ohne Gruppe" ? 1 : b[0] === "Ohne Gruppe" ? -1 : a[0].localeCompare(b[0], "de"));
}
