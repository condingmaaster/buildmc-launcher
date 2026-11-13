import { useEffect, useMemo, useState } from "react";
import * as api from "../lib/api";
import { useApp } from "../lib/store";
import { Icon } from "../components/Icon";
import { ProjectIcon } from "../components/Avatar";
import { Badge, Button, Switch } from "../components/ui";
import { AiTrace } from "./AiTrace";
import { compact, num } from "../lib/format";
import type { AssistantQuestion, AssistantSession } from "../lib/types";
import "./assistant.css";

/* Wenn das Modell ausfällt, wird gefragt statt stillschweigend
   heruntergestuft — ein schwächeres Modell liefert schlechtere Vorschläge,
   und das soll sichtbar sein. */
interface Fallback {
  message: string;
  retry: (model: string) => Promise<void>;
}

export function Assistant() {
  const notify = useApp((s) => s.notify);
  const go = useApp((s) => s.go);
  const reload = useApp((s) => s.reloadInstances);
  const config = useApp((s) => s.config);

  const [session, setSession] = useState<AssistantSession | null>(null);
  const [open, setOpen] = useState<AssistantSession[]>([]);
  // Ob ueberhaupt ein Schluessel hinterlegt ist -- ohne ihn ruft der
  // Katalog-Zuschnitt nie ein Modell auf, und das Live-Panel duerfte dann
  // gar nicht erst "wartet auf Antwort" zeigen, weil nie eine kommt.
  const [aiConfigured, setAiConfigured] = useState(false);
  const [goal, setGoal] = useState("");
  const [busy, setBusy] = useState(false);
  const [fallback, setFallback] = useState<Fallback | null>(null);
  const [name, setName] = useState("");
  // Vorbelegt aus der zuletzt genutzten Einstellung, pro Lauf aenderbar.
  const [research, setResearch] = useState(config?.assistant_web_research ?? false);
  // Zaehlt echte KI-Aufrufe -- steuert, wann das Live-Panel seine Spur
  // leert. Nicht an "busy" gekoppelt, das feuert auch bei Klicks, die
  // die KI gar nicht anfragen.
  const [aiRun, setAiRun] = useState(0);

  useEffect(() => {
    void api.assistant.openSessions().then(setOpen).catch(() => setOpen([]));
    void api.ai.status().then((s) => setAiConfigured(s.configured)).catch(() => setAiConfigured(false));
  }, []);

  /* Ein nicht erreichbares Modell ist kein gewöhnlicher Fehler: es lässt
     sich durch ein anderes beheben, und darüber entscheidet der Nutzer. */
  async function guarded(label: string, run: (model?: string) => Promise<AssistantSession>) {
    setBusy(true);
    try {
      setSession(await run());
    } catch (e) {
      const text = String(e);
      if (text.includes("nicht verfügbar")) {
        setFallback({
          message: text,
          retry: async (model) => {
            setBusy(true);
            try {
              setSession(await run(model));
              setFallback(null);
            } catch (err) {
              notify({ kind: "risk", title: label, detail: String(err) });
            } finally {
              setBusy(false);
            }
          },
        });
      } else {
        notify({ kind: "risk", title: label, detail: text });
      }
    } finally {
      setBusy(false);
    }
  }

  const start = () => {
    setAiRun((n) => n + 1);
    return guarded("Assistent konnte nicht starten",
                    (m) => api.assistant.start(goal, m, research));
  };

  async function answer(question: AssistantQuestion, choices: string[]) {
    setBusy(true);
    try {
      const next = await api.assistant.answer(session!.id, question.id, choices);
      setSession(next);
      // Ist alles beantwortet, gleich die Vorschläge holen — ohne Modell,
      // also ohne Wartezeit am Ratenlimit.
      if (next.step === "choose" && !next.searched) {
        setSession(await api.assistant.candidates(next.id));
      }
    } catch (e) {
      notify({ kind: "risk", title: "Antwort abgelehnt", detail: String(e) });
    } finally {
      setBusy(false);
    }
  }

  const buildPlan = () => {
    setAiRun((n) => n + 1);
    return guarded("Plan konnte nicht erstellt werden",
                    (m) => api.assistant.plan(session!.id, name, m));
  };

  /* Springt zurueck zur Versionsfrage, ohne einen neuen Sitzungslauf zu
     brauchen -- die bereits gegebenen Antworten bleiben erhalten, nur die
     Version gilt wieder als offen. Rein clientseitig: die eigentliche
     Antwort wird erst beim erneuten Beantworten persistiert. */
  function changeVersion() {
    if (!session) return;
    const { version: _, ...rest } = session.answers;
    setSession({
      ...session, step: "answer", answers: rest, searched: false, candidates: [],
    });
  }

  async function apply() {
    setBusy(true);
    try {
      const instance = await api.assistant.apply(session!.id);
      await reload();
      notify({ kind: "ok", title: `${instance.name} wurde angelegt` });
      go({ name: "instance", id: instance.id });
    } catch (e) {
      notify({ kind: "risk", title: "Anlegen fehlgeschlagen", detail: String(e) });
    } finally {
      setBusy(false);
    }
  }

  const current = useMemo(() => {
    if (!session || session.step !== "answer") return null;
    const vanilla = session.answers.loader?.includes("vanilla");
    return session.questions.find(
      (q) => !session.answers[q.id] && !(vanilla && ["ziel", "komfort"].includes(q.id)),
    ) ?? null;
  }, [session]);

  return (
    <>
      <header className="topbar">
        <div className="topbar__lead">
          <h1 className="topbar__title">Assistent</h1>
          {session && (
            <Badge tone={session.source === "modell" ? "accent" : "neutral"}>
              {session.source === "modell" ? "zugeschnitten" : "Standardfragen"}
            </Badge>
          )}
        </div>
        <div className="topbar__spacer" />
        {session && (
          <Button icon="close" onClick={() => {
            void api.assistant.discard(session.id);
            setSession(null);
            setGoal("");
          }}>
            Abbrechen
          </Button>
        )}
      </header>

      <div className="asst__row">
      <div className="asst u-scroll">
        {!session && (
          <Describe
            goal={goal} onGoal={setGoal} busy={busy} onStart={start}
            research={research} onResearch={setResearch}
            open={open}
            onResume={(id) => void api.assistant.load(id).then(setSession)}
          />
        )}

        {session && <Steps step={session.step} />}

        {session?.notes.map((n, i) => (
          <p className="asst__note u-rise" key={i}>
            <Icon name="spark" size={14} />{n}
          </p>
        ))}

        {current && (
          <QuestionCard
            key={current.id}
            question={current}
            busy={busy}
            onAnswer={(choices) => void answer(current, choices)}
          />
        )}

        {session?.step === "choose" && (
          <Choose
            session={session} busy={busy} name={name} onName={setName}
            onToggle={(id) => {
              const next = session.selected.includes(id)
                ? session.selected.filter((s) => s !== id)
                : [...session.selected, id];
              void api.assistant.select(session.id, next).then(setSession);
            }}
            onContinue={buildPlan}
            onRetry={() => guarded("Suche fehlgeschlagen",
                                   () => api.assistant.candidates(session.id))}
            onChangeVersion={changeVersion}
          />
        )}

        {session?.step === "review" && session.plan && (
          <Review plan={session.plan} busy={busy} onApply={apply} />
        )}
      </div>

      {/* Vor der ersten Sitzung ist "modell" vs. "katalog" noch nicht
          bekannt aus der Sitzung selbst -- aber der Schluesselstatus steht
          vorher schon fest. Ohne Schluessel ruft der Katalog-Zuschnitt nie
          ein Modell auf, und das Panel darf dann gar nicht erst auftauchen
          -- es gaebe nie eine Antwort, auf die es "wartet". */}
      {(session ? session.source === "modell" : aiConfigured) && (
        <AiTrace active={busy} resetKey={aiRun} />
      )}
      </div>

      {fallback && (
        <FallbackDialog
          message={fallback.message}
          busy={busy}
          onPick={(m) => void fallback.retry(m)}
          onCancel={() => setFallback(null)}
        />
      )}
    </>
  );
}

/* ---- Beschreibung ---------------------------------------------------- */

function Describe({ goal, onGoal, busy, onStart, open, onResume, research, onResearch }: {
  goal: string; onGoal: (v: string) => void; busy: boolean; onStart: () => void;
  open: AssistantSession[]; onResume: (id: string) => void;
  research: boolean; onResearch: (v: boolean) => void;
}) {
  const [erweitert, setErweitert] = useState(false);
  return (
    <section className="asst__intro u-rise">
      <h2 className="asst__h">Was hast du vor?</h2>
      <p className="asst__lead">
        Beschreib in eigenen Worten, worum es gehen soll — ruhig mehrere Absätze.
        Je genauer, desto besser passen die Fragen danach. Ist ein Schlüssel
        hinterlegt, wird der Fragebogen darauf zugeschnitten; sonst bekommst du
        die Standardfragen, die genauso zu einer fertigen Instanz führen.
      </p>

      <textarea
        className="input asst__goal"
        rows={7}
        placeholder={"Zum Beispiel:\n\nIch will mit zwei Freunden ein Technik-Paket spielen.\nMein Laptop ist eher schwach, Bildrate geht vor Optik.\nEine Minikarte wäre schön, Shader brauche ich nicht."}
        value={goal}
        onChange={(e) => onGoal(e.target.value)}
      />

      <button className="asst__advanced-toggle" onClick={() => setErweitert((v) => !v)}
              aria-expanded={erweitert}>
        <Icon name="chevron" size={12} />
        Erweitert
      </button>
      {erweitert && (
        <div className="asst__advanced u-rise">
          <div className="asst__advanced-row">
            <span>
              <strong>Web-Recherche nutzen</strong>
              <span className="asst__hint">
                Dauert länger, trifft aber aktuelle Empfehlungen besser — das Modell darf den Fragebogen dafür freier gestalten, nicht nur den Standardkatalog kürzen.
              </span>
            </span>
            <Switch checked={research} onChange={onResearch} label="Web-Recherche nutzen" />
          </div>
        </div>
      )}

      <div className="asst__actions">
        <Button tone="primary" size="lg" icon="spark" busy={busy}
                disabled={!goal.trim()} onClick={onStart}>
          Loslegen
        </Button>
        <span className="asst__hint">
          {goal.trim().split(/\s+/).filter(Boolean).length} Wörter
        </span>
      </div>

      {open.length > 0 && (
        <div className="asst__open">
          <h3 className="asst__h3">Angefangen</h3>
          {open.map((s) => (
            <button className="asst__resume" key={s.id} onClick={() => onResume(s.id)}>
              <Icon name="clock" size={15} />
              <span className="u-truncate">{s.goal || "Ohne Beschreibung"}</span>
              <Badge>{Object.keys(s.answers).length} beantwortet</Badge>
            </button>
          ))}
        </div>
      )}
    </section>
  );
}

/* ---- Fortschritt ----------------------------------------------------- */

const STEPS: [AssistantSession["step"], string][] = [
  ["answer", "Fragen"],
  ["choose", "Auswahl"],
  ["review", "Plan"],
  ["done", "Fertig"],
];

function Steps({ step }: { step: AssistantSession["step"] }) {
  const at = STEPS.findIndex(([s]) => s === step);
  return (
    <ol className="steps">
      {STEPS.map(([s, label], i) => (
        <li key={s} className={`steps__item${i < at ? " is-done" : ""}${i === at ? " is-now" : ""}`}>
          <span className="steps__dot">{i < at ? <Icon name="check" size={12} /> : i + 1}</span>
          {label}
        </li>
      ))}
    </ol>
  );
}

/* ---- Eine Frage ------------------------------------------------------ */

function QuestionCard({ question, busy, onAnswer }: {
  question: AssistantQuestion; busy: boolean; onAnswer: (choices: string[]) => void;
}) {
  const [picked, setPicked] = useState<string[]>([]);

  const toggle = (id: string) => {
    if (!question.multi) {
      onAnswer([id]);
      return;
    }
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id]));
  };

  return (
    <section className="qcard u-rise">
      <h2 className="qcard__prompt">{question.prompt}</h2>
      <p className="qcard__why">{question.why}</p>

      <div className="qcard__options">
        {question.options.map((o) => (
          <button
            key={o.id}
            className={`qopt${picked.includes(o.id) ? " is-on" : ""}`}
            onClick={() => toggle(o.id)}
            disabled={busy}
          >
            <span className="qopt__mark">
              {picked.includes(o.id) && <Icon name="check" size={13} />}
            </span>
            <span className="qopt__text">
              <strong>{o.label}</strong>
              {o.detail && <span>{o.detail}</span>}
            </span>
          </button>
        ))}
      </div>

      {question.multi && (
        <Button tone="primary" icon="chevron" busy={busy}
                disabled={!picked.length} onClick={() => onAnswer(picked)}>
          Weiter
        </Button>
      )}
    </section>
  );
}

/* ---- Auswahl --------------------------------------------------------- */

function Choose({ session, busy, name, onName, onToggle, onContinue, onRetry, onChangeVersion }: {
  session: AssistantSession; busy: boolean; name: string;
  onName: (v: string) => void; onToggle: (id: string) => void;
  onContinue: () => void; onRetry: () => void; onChangeVersion: () => void;
}) {
  // Eine leere Liste allein sagt nichts: sie bedeutet „sucht noch" *oder*
  // „nichts gefunden". Nur das Backend weiß, welches von beidem gilt.
  if (!session.searched) {
    return (
      <div className="asst__loading">
        <span className="btn__spin" />
        <p>Vorschläge werden gesucht…</p>
      </div>
    );
  }

  if (!session.candidates.length) {
    return (
      <section className="asst__empty u-rise">
        <Icon name="search" size={30} />
        <h2 className="asst__h">Dazu wurde nichts gefunden</h2>
        <p className="asst__lead">
          Zu diesen Antworten gibt es nichts, was zugleich zur gewählten
          Minecraft-Version und zum Lader passt. Eine etwas ältere Version ist
          meistens der schnellste Weg — dort gibt es fast alles.
        </p>
        {session.notes.map((n, i) => (
          <p key={i} className="asst__note">{n}</p>
        ))}
        <div className="asst__empty-actions">
          <Button tone="primary" icon="refresh" busy={busy} onClick={onRetry}>
            Erneut suchen
          </Button>
          <Button tone="ghost" icon="chevron" onClick={onChangeVersion}>
            Andere Version wählen
          </Button>
        </div>
      </section>
    );
  }

  return (
    <section className="asst__choose u-rise">
      <h2 className="asst__h">Das passt zu deinen Antworten</h2>
      <p className="asst__lead">
        Alle Vorschläge stammen aus einer echten Suche — sie existieren und
        passen zu Version und Loader. Mehrfach getroffene sind vorausgewählt.
      </p>

      <div className="cands u-stagger">
        {session.candidates.map((c) => {
          const on = session.selected.includes(c.project_id);
          return (
            <button key={c.project_id} className={`cand${on ? " is-on" : ""}`}
                    onClick={() => onToggle(c.project_id)} disabled={busy}>
              <ProjectIcon title={c.title} url={c.icon_url} size={38} />
              <span className="cand__body">
                <span className="cand__title u-truncate">{c.title}</span>
                <span className="cand__desc u-clamp-2">{c.description}</span>
                <span className="cand__meta">
                  <Icon name="download" size={11} />{compact(c.downloads)}
                  {c.matched.length > 1 && <Badge tone="accent">passt mehrfach</Badge>}
                  {!c.version_exact && (
                    <Badge tone="warn" title="Nur gefunden, weil die Versionsprüfung \
gelockert wurde — passt möglicherweise nicht exakt.">
                      evtl. andere Version
                    </Badge>
                  )}
                </span>
              </span>
              <span className="cand__mark">{on && <Icon name="check" size={15} />}</span>
            </button>
          );
        })}
      </div>

      <div className="asst__finish">
        <label className="field">
          <span className="field__label">Name der Instanz</span>
          <input className="input" value={name} onChange={(e) => onName(e.target.value)}
                 placeholder="wird sonst aus Version und Loader gebildet" />
        </label>
        <Button tone="primary" size="lg" icon="spark" busy={busy} onClick={onContinue}>
          {num(session.selected.length)} übernehmen und planen
        </Button>
      </div>
    </section>
  );
}

/* ---- Plan ------------------------------------------------------------ */

function Review({ plan, busy, onApply }: {
  plan: NonNullable<AssistantSession["plan"]>; busy: boolean; onApply: () => void;
}) {
  const gewaehlt = plan.mods.filter((m) => !m.from_optimization);
  const optimierung = plan.mods.filter((m) => m.from_optimization);

  return (
    <section className="plan u-rise">
      <h2 className="asst__h">{plan.name}</h2>

      {plan.rationale && <p className="plan__rationale">{plan.rationale}</p>}

      <div className="plan__facts">
        <Fact label="Minecraft" value={plan.mc_version} />
        <Fact label="Loader" value={plan.loader} sub={plan.loader_version ?? undefined} />
        <Fact label="Arbeitsspeicher" value={`${(plan.max_ram / 1024).toFixed(0)} GB`}
              sub={`min. ${(plan.min_ram / 1024).toFixed(0)} GB`} />
        <Fact label="Optimierung" value={plan.optimization ?? "keine"}
              sub={plan.frame_cap ? `${plan.frame_cap} FPS, VSync` : "ungebremst"} />
      </div>

      {plan.notes.map((n, i) => (
        <p className="plan__warn" key={i}><Icon name="warn" size={14} />{n}</p>
      ))}

      {gewaehlt.length > 0 && <ModGroup title="Deine Auswahl" mods={gewaehlt} />}
      {optimierung.length > 0 && (
        <ModGroup title="Für die Optimierung" mods={optimierung} muted />
      )}

      <details className="plan__flags">
        <summary>JVM-Schalter</summary>
        <code>{plan.jvm_args.join(" ")}</code>
      </details>

      <div className="asst__finish">
        <Button tone="primary" size="lg" icon="check" busy={busy} onClick={onApply}>
          Anlegen und installieren
        </Button>
      </div>
    </section>
  );
}

function Fact({ label, value, sub }: { label: string; value: string; sub?: string }) {
  return (
    <div className="stat">
      <span className="stat__label">{label}</span>
      <strong className="stat__value">{value}</strong>
      {sub && <span className="stat__sub">{sub}</span>}
    </div>
  );
}

function ModGroup({ title, mods, muted }: {
  title: string; mods: NonNullable<AssistantSession["plan"]>["mods"]; muted?: boolean;
}) {
  return (
    <div className={`plan__group${muted ? " is-muted" : ""}`}>
      <h3 className="asst__h3">{title} <span>{num(mods.length)}</span></h3>
      <ul className="plan__mods">
        {mods.map((m) => (
          <li key={m.project_id}>
            <ProjectIcon title={m.title} url={m.icon_url} size={30} />
            <div className="mod__id">
              <span className="mod__name">{m.title}</span>
              <span className="plan__reason">{m.reason}</span>
            </div>
          </li>
        ))}
      </ul>
    </div>
  );
}

/* ---- Ersatzmodell ---------------------------------------------------- */

function FallbackDialog({ message, busy, onPick, onCancel }: {
  message: string; busy: boolean; onPick: (model: string) => void; onCancel: () => void;
}) {
  const [models, setModels] = useState<{ id: string }[]>([]);
  useEffect(() => { void api.ai.models().then(setModels).catch(() => setModels([])); }, []);

  return (
    <div className="sheet__scrim" onClick={onCancel}>
      <div className="fbdlg" onClick={(e) => e.stopPropagation()} role="dialog">
        <Icon name="warn" size={22} />
        <h3>Modell nicht erreichbar</h3>
        <p className="fbdlg__msg">{message}</p>
        <p className="fbdlg__note">
          Ein anderes Modell hat eigene Grenzen und kommt womöglich durch.
          Es liefert aber unter Umständen schlechtere Vorschläge — deshalb
          wird gefragt statt stillschweigend gewechselt.
        </p>
        <div className="fbdlg__models">
          {models.slice(0, 4).map((m) => (
            <Button key={m.id} tone="outline" busy={busy} onClick={() => onPick(m.id)}>
              {m.id}
            </Button>
          ))}
        </div>
        <div className="fbdlg__foot">
          <Button tone="ghost" onClick={onCancel}>Ohne Modell weitermachen</Button>
        </div>
      </div>
    </div>
  );
}
