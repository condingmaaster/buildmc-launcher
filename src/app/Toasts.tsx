import { useApp } from "../lib/store";
import { Icon, type IconName } from "../components/Icon";

const ICON: Record<string, IconName> = {
  info: "spark", ok: "check", warn: "warn", risk: "warn",
};

export function Toasts() {
  const toasts = useApp((s) => s.toasts);
  const dismiss = useApp((s) => s.dismiss);

  return (
    <div className="toasts" role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`toast toast--${t.kind}`}>
          <Icon name={ICON[t.kind] ?? "spark"} size={17} />
          <div className="toast__body">
            <strong>{t.title}</strong>
            {t.detail && <span>{t.detail}</span>}
          </div>
          <button className="toast__x" onClick={() => dismiss(t.id)} aria-label="Schliessen">
            <Icon name="close" size={14} />
          </button>
        </div>
      ))}
    </div>
  );
}
