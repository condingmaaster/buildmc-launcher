import { useApp } from "../lib/store";
import { bytes } from "../lib/format";
import { Icon } from "../components/Icon";

/** Echte Fortschrittsanzeige fuer laufende Downloads (Version, Mod, Modpack).
 *
 *  Das Backend meldet seit jeher Prozent, Geschwindigkeit und Dateiname --
 *  bisher wurde das nirgends abgeholt, und ein grosser Download (Java,
 *  tausende Assets) zeigte deshalb nur einen Knopf-Spinner ohne jede Zahl.
 *  Das sah bei einer laengeren Wartezeit wie eingefroren aus, obwohl der
 *  Launcher laengst arbeitete. */
export function DownloadBar() {
  const p = useApp((s) => s.progress);
  if (!p || p.total === 0) return null;

  const pct = Math.min(100, Math.round((p.current / p.total) * 100));

  return (
    <div className="dlbar" role="status" aria-live="polite">
      <Icon name={p.done ? "check" : "download"} size={15} />
      <div className="dlbar__body">
        <div className="dlbar__row">
          <span className="dlbar__label">{p.label}</span>
          <span className="dlbar__stat">
            {p.done ? "fertig" : `${p.current}/${p.total} · ${bytes(p.bytes_per_second)}/s`}
          </span>
        </div>
        <div className="dlbar__track">
          <div className="dlbar__fill" style={{ width: `${p.done ? 100 : pct}%` }} />
        </div>
        {!p.done && p.detail && <span className="dlbar__detail u-truncate">{p.detail}</span>}
      </div>
    </div>
  );
}
