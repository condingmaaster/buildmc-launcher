import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { ProjectIcon } from "../components/Avatar";
import { Badge, Button, Empty, Segmented } from "../components/ui";
import { ago, compact } from "../lib/format";
import type { AiStatus, ProjectType, SearchIntent, SearchResult } from "../lib/types";
import "./browse.css";

/* Modrinth und CurseForge liefern beide 20 Treffer je Seite -- beide
   werden mit derselben Seitenzahl abgefragt (siehe commands.rs), daher
   gilt dieselbe Groesse fuer die Fortschrittsanzeige unten. */
const PER_PAGE = 20;

const TYPES: { value: ProjectType; label: string }[] = [
  { value: "mod",          label: "Mods" },
  { value: "modpack",      label: "Modpacks" },
  { value: "resourcepack", label: "Resourcepacks" },
  { value: "shader",       label: "Shader" },
  { value: "datapack",     label: "Datapacks" },
];

/* "Passend" filtert auf Version und Loader der Zielinstanz, "Alle" sucht
   ohne diese Filter -- sonst findet man Mods, die (noch) nicht fuer die
   eigene Version da sind, ueberhaupt nicht. */
type Scope = "passend" | "alle";
const SCOPES: { value: Scope; label: string }[] = [
  { value: "passend", label: "Passend" },
  { value: "alle",    label: "Alle Versionen" },
];

/* Passt ein Treffer zur Instanz? Nur fuer den Hinweis im Modus "Alle" --
   ein leeres Feld vom Portal zaehlt als "unbekannt", nicht als "passt nicht". */
function fits(r: SearchResult, mc: string, loader: string): boolean {
  if (r.game_versions.length && !r.game_versions.includes(mc)) return false;
  if (r.project_type === "shader" || r.project_type === "resourcepack" || r.project_type === "datapack") return true;
  return !r.loaders.length || r.loaders.includes(loader as SearchResult["loaders"][number]);
}

export function Browse() {
  const instances = useApp((s) => s.instances);
  const notify = useApp((s) => s.notify);

  const [query, setQuery] = useState("");
  const [type, setType] = useState<ProjectType>("mod");
  const [scope, setScope] = useState<Scope>("passend");
  const [target, setTarget] = useState<string>(instances[0]?.id ?? "");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [installing, setInstalling] = useState<string | null>(null);
  const [selected, setSelected] = useState<SearchResult | null>(null);

  /* Die Assistenz hat ein eigenes Feld. Sie deutet eine Absicht und setzt
     daraus Suchbegriff und Filter -- gesucht wird danach ganz normal bei
     den Portalen. Ein gemeinsames Feld mit der Stichwortsuche wuerde
     beides verwaschen: "sodium" ist kein Wunsch, sondern ein Name. */
  const [ai, setAi] = useState<AiStatus | null>(null);
  const [wish, setWish] = useState("");
  const [thinking, setThinking] = useState(false);
  const [intent, setIntent] = useState<SearchIntent | null>(null);

  useEffect(() => { void api.ai.status().then(setAi).catch(() => setAi(null)); }, []);

  const inst = instances.find((i) => i.id === target);
  const alle = scope === "alle";
  const mcFilter = alle ? null : inst?.mc_version ?? null;
  const loaderFilter = alle || type === "shader" ? null : inst?.loader ?? null;

  /* Ein neuer Suchbegriff, Inhaltstyp oder Ziel faengt wieder bei Seite 0
     an -- sonst stuende man nach dem Umschalten mitten in einer Trefferliste,
     die es fuer die neue Suche vielleicht gar nicht mehr gibt. */
  useEffect(() => { setPage(0); }, [query, type, mcFilter, loaderFilter]);

  /* Eingabe entprellen: erst 280 ms nach dem letzten Tastendruck suchen.
     Das haelt die Trefferliste ruhig und spart Anfragen. Seite wechseln
     braucht keine Entprellung -- das ist schon ein bewusster Klick. */
  const timer = useRef<number>();
  useEffect(() => {
    window.clearTimeout(timer.current);
    let cancelled = false;
    setLoading(true);
    timer.current = window.setTimeout(() => {
      void api.mods
        .search(query, mcFilter, loaderFilter, type, page)
        .then((r) => { if (!cancelled) { setResults(r.results); setTotal(r.total); } })
        .catch(() => { if (!cancelled) { setResults([]); setTotal(0); } })
        .finally(() => { if (!cancelled) setLoading(false); });
    }, 280);
    return () => { cancelled = true; window.clearTimeout(timer.current); };
  }, [query, type, mcFilter, loaderFilter, page]);

  async function install(r: SearchResult) {
    if (!inst) {
      notify({ kind: "warn", title: "Keine Zielinstanz", detail: "Wähle oben aus, wohin installiert wird." });
      return;
    }
    setInstalling(r.project_id);
    try {
      const versions = await api.mods.versions(r.project_id, inst.mc_version, r.project_type === "shader" ? null : inst.loader, r.provider);
      const best = versions.find((v) => v.release_type === "release") ?? versions[0];
      if (!best) {
        notify({ kind: "warn", title: `${r.title}: keine passende Version`,
                 detail: `Für ${inst.mc_version}${r.project_type === "shader" ? "" : ` · ${inst.loader}`} gibt es keine Datei. Wähle eine andere Instanz als Ziel.` });
        return;
      }

      if (r.project_type === "shader") {
        await api.mods.installShader(inst.id, best.version_id);
        notify({ kind: "ok", title: `${r.title} installiert`,
                 detail: `in ${inst.name} · Im Spiel unter Shader auswählen.` });
        return;
      }

      if (r.provider === "modrinth") {
        /* Erst planen, dann anwenden -- so sieht der Nutzer Abhaengigkeiten
           und Konflikte, bevor etwas geschrieben wird. Die Aufloesung
           kennt bislang nur Modrinth. */
        const plan = await api.mods.resolve(inst.id, best.version_id);
        if (plan?.conflicts?.length) {
          notify({ kind: "warn", title: `${r.title}: Konflikt`,
                   detail: plan.conflicts[0]!.detail });
          return;
        }
      }
      await api.mods.install(inst.id, best.version_id, r.provider);
      notify({ kind: "ok", title: `${r.title} installiert`,
               detail: `in ${inst.name}` });
    } catch (e) {
      notify({ kind: "risk", title: "Installation fehlgeschlagen", detail: String(e) });
    } finally {
      setInstalling(null);
    }
  }

  async function askAssistant(e: React.FormEvent) {
    e.preventDefault();
    const text = wish.trim();
    if (!text) return;
    setThinking(true);
    try {
      const got = await api.ai.interpret(text);
      if (!got) {
        notify({ kind: "warn", title: "Kein Schlüssel hinterlegt",
                 detail: "Unter Einstellungen → Assistent eintragen." });
        return;
      }
      setIntent(got);
      if (got.project_type) setType(got.project_type);
      setQuery(got.queries[0] ?? text);
    } catch (err) {
      notify({ kind: "risk", title: "Assistent nicht erreichbar", detail: String(err) });
    } finally {
      setThinking(false);
    }
  }

  const empty = !loading && !results.length;

  return (
    <>
      <header className="topbar">
        <div className="topbar__lead">
          <h1 className="topbar__title">Entdecken</h1>
        </div>

        <div className="topbar__search browse__search">
          <Icon name="search" size={16} />
          <input
            className="input" type="search" autoFocus
            placeholder={type === "shader" ? "Shaderpakete suchen, z. B. Complementary…" : "Modrinth und CurseForge durchsuchen…"}
            value={query} onChange={(e) => setQuery(e.target.value)}
            aria-label="Inhalte suchen"
          />
        </div>

        <label className="browse__target">
          <span>Ziel</span>
          <select className="input" value={target}
                  onChange={(e) => setTarget(e.target.value)}
                  aria-label="Zielinstanz">
            {instances.map((i) => (
              <option key={i.id} value={i.id}>
                {i.name} · {i.mc_version}
              </option>
            ))}
          </select>
        </label>
      </header>

      <div className="browse__filters">
        <Segmented label="Inhaltstyp" value={type} onChange={setType} options={TYPES} />
        <Segmented label="Versionen" value={scope} onChange={setScope} options={SCOPES} />
        {alle ? (
          <span className="browse__scope">ohne Versions- und Loader-Filter</span>
        ) : inst && (
          <span className="browse__scope">
            {type === "shader" ? "Shaderpakete für " : "gefiltert auf "}
            <strong>{inst.mc_version}</strong>
            {type === "shader" ? " · benötigt Iris oder OptiFine" : <> · <strong>{inst.loader}</strong></>}
          </span>
        )}
      </div>

      <form className={`assist${ai?.configured ? "" : " is-off"}`} onSubmit={askAssistant}>
        <Icon name="spark" size={16} />
        <input
          className="input"
          placeholder={ai?.configured
            ? "Beschreib, was du suchst — z. B. „mehr Bildrate und Shader, muss mit Fabric laufen“"
            : "Assistenz-Suche: Schlüssel unter Einstellungen → Assistent hinterlegen"}
          value={wish}
          onChange={(e) => setWish(e.target.value)}
          disabled={!ai?.configured}
          aria-label="Wunsch beschreiben"
        />
        <Button tone="primary" size="sm" type="submit"
                busy={thinking} disabled={!ai?.configured || !wish.trim()}>
          Vorschlagen
        </Button>
      </form>

      {intent && (
        <div className="assist__read u-rise">
          <Icon name="spark" size={13} />
          <span>{intent.reading}</span>
          <div className="assist__chips">
            {intent.queries.map((q) => (
              <button key={q} className={`assist__chip${q === query ? " is-on" : ""}`}
                      onClick={() => setQuery(q)}>
                {q}
              </button>
            ))}
          </div>
          <button className="assist__x" onClick={() => { setIntent(null); setQuery(""); }}
                  aria-label="Vorschlag verwerfen">
            <Icon name="close" size={13} />
          </button>
        </div>
      )}

      <div className="browse u-scroll">
        {loading && (
          <div className="browse__grid">
            {Array.from({ length: 6 }, (_, i) => <div className="skel" key={i} />)}
          </div>
        )}

        {empty && (
          <Empty icon="compass" title="Keine Treffer"
                 detail={query
                   ? `Nichts gefunden für "${query}".`
                   : type === "shader" ? "Keine Shaderpakete für diese Minecraft-Version gefunden."
                   : "Tippe, um Mods, Resourcepacks, Shader und Datapacks zu durchsuchen."} />
        )}

        {!loading && results.length > 0 && (
          <>
          <div className="browse__grid u-stagger">
            {results.map((r) => (
              <article className="card" key={`${r.provider}-${r.project_id}`}>
                <div className="card__top">
                  <ProjectIcon title={r.title} url={r.icon_url} size={42} />
                  <div className="card__id">
                    <h3 className="card__title u-truncate">{r.title}</h3>
                    <span className="card__author u-truncate">von {r.author}</span>
                  </div>
                  <Badge tone={r.provider === "modrinth" ? "ok" : "warn"}>
                    {r.provider === "modrinth" ? "Modrinth" : "CurseForge"}
                  </Badge>
                </div>

                <p className="card__desc u-clamp-2">{r.description}</p>

                <div className="card__meta">
                  <span><Icon name="download" size={12} />{compact(r.downloads)}</span>
                  <span><Icon name="clock" size={12} />{ago(r.updated_at)}</span>
                  {r.server_side === "required" && <Badge>serverseitig</Badge>}
                  {alle && inst && !fits(r, inst.mc_version, inst.loader) && (
                    <Badge tone="warn" title={`Laut Portal nicht für ${inst.name}`}>
                      nicht für {inst.mc_version}
                    </Badge>
                  )}
                </div>

                <div className="card__actions">
                  <Button tone="primary" size="sm" icon="download"
                          busy={installing === r.project_id}
                          onClick={() => void install(r)}>
                    Installieren
                  </Button>
                  <Button tone="ghost" size="sm" onClick={() => setSelected(r)}>
                    Details
                  </Button>
                </div>
              </article>
            ))}
          </div>

          <nav className="browse__pager">
            <Button tone="ghost" icon="chevron" disabled={page === 0}
                    onClick={() => setPage((p) => Math.max(0, p - 1))}>
              Zurück
            </Button>
            <span className="browse__pager-info">
              Seite {page + 1} von {Math.max(1, Math.ceil(total / PER_PAGE))} · {compact(total)} Treffer
            </span>
            <Button tone="ghost" icon="chevron"
                    disabled={(page + 1) * PER_PAGE >= total}
                    onClick={() => setPage((p) => p + 1)}>
              Weiter
            </Button>
          </nav>
          </>
        )}
      </div>

      {selected && <Details item={selected} onClose={() => setSelected(null)} />}
    </>
  );
}

/* ---- Detailblatt ----------------------------------------------------- */

function Details({ item, onClose }: { item: SearchResult; onClose: () => void }) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [onClose]);

  return (
    <div className="sheet__scrim" onClick={onClose}>
      <aside className="sheet" onClick={(e) => e.stopPropagation()}
             role="dialog" aria-label={item.title}>
        <header className="sheet__head">
          <ProjectIcon title={item.title} url={item.icon_url} size={42} />
          <div className="card__id">
            <h2>{item.title}</h2>
            <span className="card__author">von {item.author}</span>
          </div>
          <button className="sheet__x" onClick={onClose} aria-label="Schliessen">
            <Icon name="close" size={16} />
          </button>
        </header>

        <p className="sheet__desc">{item.description}</p>

        <dl className="sheet__facts">
          <div><dt>Downloads</dt><dd>{compact(item.downloads)}</dd></div>
          <div><dt>Folgen</dt><dd>{compact(item.follows)}</dd></div>
          <div><dt>Aktualisiert</dt><dd>{ago(item.updated_at)}</dd></div>
          <div><dt>Lizenz</dt><dd>{item.license ?? "unbekannt"}</dd></div>
          <div><dt>Client</dt><dd>{item.client_side}</dd></div>
          <div><dt>Server</dt><dd>{item.server_side}</dd></div>
        </dl>

        <div className="sheet__chips">
          {item.game_versions.map((v) => <Badge key={v}>{v}</Badge>)}
          {item.loaders.map((l) => <Badge key={l} tone="accent">{l}</Badge>)}
        </div>
      </aside>
    </div>
  );
}
