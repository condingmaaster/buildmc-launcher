/* =========================================================================
   Live-Spur eines Modellaufrufs.

   Zeigt, was die KI gerade tut -- Denkschritt, Websuche, Ruckfaelle --
   waehrend es passiert. Anders als die Spielkonsole (die feste Zeilen
   bekommt) traegt dieses Panel Ereignisse zusammen: mehrere
   Denkschritt-Haeppchen werden zu einem wachsenden Absatz, eine
   Werkzeugausfuehrung von "gestartet" bis "fertig" zu einer Karte.
   ========================================================================= */

import { useEffect, useRef, useState } from "react";
import * as api from "../lib/api";
import { Icon } from "../components/Icon";
import type { AiLogEvent } from "../lib/types";
import "./aitrace.css";

interface Werkzeuglauf {
  name: string;
  args: string;
  fertig: boolean;
  query: string | null;
  ergebnisse: { title: string; domain: string; url: string }[];
}

interface Eintrag {
  key: number;
  art: "thinking" | "writing" | "tool" | "retry" | "done";
  text?: string;
  werkzeug?: Werkzeuglauf;
  tokens?: { prompt: number; completion: number };
}

/** Fasst ein Rohereignis in die wachsende Eintragsliste ein -- Denken und
    eine laufende Werkzeugausfuehrung werden fortgeschrieben statt jedes
    Mal eine neue Zeile anzuhaengen. */
function reduziere(bisher: Eintrag[], ev: AiLogEvent, naechsterSchluessel: () => number): Eintrag[] {
  const letzter = bisher[bisher.length - 1];

  switch (ev.kind) {
    case "thinking": {
      if (letzter?.art === "thinking") {
        return [...bisher.slice(0, -1), { ...letzter, text: (letzter.text ?? "") + ev.text }];
      }
      return [...bisher, { key: naechsterSchluessel(), art: "thinking", text: ev.text }];
    }
    case "writing":
      return [...bisher, { key: naechsterSchluessel(), art: "writing" }];
    case "tool_started":
      return [...bisher, {
        key: naechsterSchluessel(), art: "tool",
        werkzeug: { name: ev.name, args: "", fertig: false, query: null, ergebnisse: [] },
      }];
    case "tool_args": {
      if (letzter?.art === "tool" && letzter.werkzeug && !letzter.werkzeug.fertig) {
        const werkzeug = { ...letzter.werkzeug, args: letzter.werkzeug.args + ev.delta };
        return [...bisher.slice(0, -1), { ...letzter, werkzeug }];
      }
      return bisher;
    }
    case "tool_done": {
      if (letzter?.art === "tool" && letzter.werkzeug) {
        const werkzeug: Werkzeuglauf = {
          ...letzter.werkzeug, fertig: true, query: ev.query, ergebnisse: ev.results,
        };
        return [...bisher.slice(0, -1), { ...letzter, werkzeug }];
      }
      return bisher;
    }
    case "retry":
      return [...bisher, { key: naechsterSchluessel(), art: "retry", text: ev.reason }];
    case "done":
      return [...bisher, {
        key: naechsterSchluessel(), art: "done",
        tokens: { prompt: ev.prompt_tokens, completion: ev.completion_tokens },
      }];
    default:
      return bisher;
  }
}

const SCHRITT_LABEL: Record<string, string> = {
  katalog: "Fragebogen wird zugeschnitten",
  plan: "Plan wird begründet",
};

export function AiTrace({ active, resetKey }: { active: boolean; resetKey: number }) {
  const [eintraege, setEintraege] = useState<Eintrag[]>([]);
  const [schritt, setSchritt] = useState<string | null>(null);
  const zaehler = useRef(0);
  const box = useRef<HTMLDivElement>(null);
  const naechsterSchluessel = () => ++zaehler.current;

  // Ein neuer Lauf faengt mit leerer Spur an -- sonst haengt am Anfang
  // eines Katalog-Zuschnitts noch die Spur des vorigen Plans. Ausgeloest
  // vom Aufrufer direkt vor jedem echten Modellaufruf, nicht am blossen
  // "busy" haengend -- das waere bei jedem Fragenklick der Fall, der die
  // KI gar nicht anspricht.
  useEffect(() => {
    setEintraege([]); setSchritt(null);
  }, [resetKey]);

  useEffect(() => {
    const off = api.events.aiLog((ev) => {
      setSchritt(ev.step);
      setEintraege((bisher) => reduziere(bisher, ev, naechsterSchluessel));
    });
    return () => { void off.then((f) => f()); };
  }, []);

  useEffect(() => {
    box.current?.scrollTo({ top: box.current.scrollHeight, behavior: "smooth" });
  }, [eintraege]);

  if (!active && eintraege.length === 0) return null;

  return (
    <aside className="aitrace u-rise">
      <header className="aitrace__head">
        <Icon name="spark" size={14} />
        <span>{schritt ? SCHRITT_LABEL[schritt] ?? schritt : "KI-Ablauf"}</span>
        {active && <span className="aitrace__pulse" />}
      </header>

      <div className="aitrace__body u-scroll" ref={box}>
        {eintraege.length === 0 && (
          <p className="aitrace__wait">Warte auf die erste Antwort…</p>
        )}
        {eintraege.map((e) => {
          if (e.art === "thinking") {
            return <p key={e.key} className="aitrace__think">{e.text}</p>;
          }
          if (e.art === "writing") {
            return (
              <p key={e.key} className="aitrace__status">
                <Icon name="check" size={12} />Antwort wird geschrieben…
              </p>
            );
          }
          if (e.art === "retry") {
            return (
              <p key={e.key} className="aitrace__retry">
                <Icon name="warn" size={12} />{e.text}
              </p>
            );
          }
          if (e.art === "done") {
            return (
              <p key={e.key} className="aitrace__done">
                {e.tokens!.prompt + e.tokens!.completion} Tokens verwendet
              </p>
            );
          }
          const w = e.werkzeug!;
          return (
            <div key={e.key} className="aitrace__tool">
              <div className="aitrace__tool-head">
                <Icon name="search" size={13} />
                {w.fertig
                  ? <span>Websuche: „{w.query ?? "…"}“</span>
                  : <span>Websuche läuft…</span>}
              </div>
              {w.fertig && w.ergebnisse.length > 0 && (
                <ul className="aitrace__hits">
                  {w.ergebnisse.map((r, i) => (
                    <li key={i}>
                      <button onClick={() => void api.system.openExternal(r.url)}>
                        <span className="aitrace__hit-title u-truncate">{r.title}</span>
                        <span className="aitrace__hit-domain">{r.domain}</span>
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {w.fertig && w.ergebnisse.length === 0 && (
                <p className="aitrace__hint">Keine verwertbaren Treffer.</p>
              )}
            </div>
          );
        })}
      </div>
    </aside>
  );
}
