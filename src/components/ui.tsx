/* Grundbausteine. Bewusst wenige, bewusst schlicht -- die Wirkung kommt
   aus Raum, Bewegung und Farbe, nicht aus einer Bauteilbibliothek. */

import type { ButtonHTMLAttributes, ReactNode } from "react";
import { Icon, type IconName } from "./Icon";

/* ---- Schaltflaeche --------------------------------------------------- */

type Tone = "primary" | "ghost" | "outline" | "danger";
type Size = "sm" | "md" | "lg";

export function Button({
  tone = "ghost", size = "md", icon, iconRight, busy, children, className = "", disabled, ...rest
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  tone?: Tone; size?: Size; icon?: IconName; iconRight?: IconName;
  busy?: boolean; children?: ReactNode;
}) {
  return (
    <button
      className={`btn btn--${tone} btn--${size}${busy ? " is-busy" : ""} ${className}`}
      {...rest}
      // Nach dem Spread, nicht davor -- `disabled` ist eigens
      // herausgeloest, damit ein vom Aufrufer mitgegebenes `disabled`
      // (z.B. `disabled={!ausgewaehlt}`) nicht die durch `busy`
      // berechnete Sperre stillschweigend ueberschreibt. Sonst bleibt der
      // Knopf waehrend eines laufenden Vorgangs anklickbar, sobald
      // gleichzeitig ein eigenes `disabled` gesetzt ist.
      disabled={disabled || busy}
    >
      {busy ? <span className="btn__spin" aria-hidden="true" />
            : icon && <Icon name={icon} size={size === "lg" ? 19 : 16} />}
      {children && <span>{children}</span>}
      {iconRight && <Icon name={iconRight} size={15} />}
    </button>
  );
}

/* ---- Abzeichen ------------------------------------------------------- */

export function Badge({ children, hue, tone = "neutral", title }: {
  children: ReactNode; hue?: number;
  tone?: "neutral" | "ok" | "warn" | "risk" | "accent"; title?: string;
}) {
  const style = hue !== undefined
    ? { "--badge-h": String(hue) } as React.CSSProperties
    : undefined;
  return (
    <span className={`badge badge--${hue !== undefined ? "hue" : tone}`}
          style={style} title={title}>
      {children}
    </span>
  );
}

/* ---- Segmentierte Auswahl ------------------------------------------- */

export function Segmented<T extends string>({ options, value, onChange, label }: {
  options: { value: T; label: string; icon?: IconName }[];
  value: T; onChange: (v: T) => void; label: string;
}) {
  return (
    <div className="seg" role="radiogroup" aria-label={label}>
      {options.map((o) => (
        <button
          key={o.value}
          role="radio"
          aria-checked={o.value === value}
          className={`seg__opt${o.value === value ? " is-on" : ""}`}
          onClick={() => onChange(o.value)}
        >
          {o.icon && <Icon name={o.icon} size={14} />}
          {o.label}
        </button>
      ))}
    </div>
  );
}

/* ---- Leerzustand ----------------------------------------------------- */

export function Empty({ icon = "spark", title, detail, action }: {
  icon?: IconName; title: string; detail?: string; action?: ReactNode;
}) {
  return (
    <div className="empty u-rise">
      <Icon name={icon} size={34} />
      <h3>{title}</h3>
      {detail && <p>{detail}</p>}
      {action}
    </div>
  );
}

/* ---- Messwert -------------------------------------------------------- */

export function Stat({ label, value, sub, hue }: {
  label: string; value: string; sub?: string; hue?: number;
}) {
  return (
    <div className="stat" style={hue !== undefined
      ? { "--stat-h": String(hue) } as React.CSSProperties : undefined}>
      <span className="stat__label">{label}</span>
      <strong className="stat__value">{value}</strong>
      {sub && <span className="stat__sub">{sub}</span>}
    </div>
  );
}

/* ---- Balken ---------------------------------------------------------- */

export function Meter({ value, max, tone = "accent", label }: {
  value: number; max: number; tone?: "accent" | "ok" | "warn" | "risk"; label?: string;
}) {
  const pct = max > 0 ? Math.min(100, (value / max) * 100) : 0;
  return (
    <div className={`meter meter--${tone}`} role="progressbar"
         aria-valuenow={Math.round(pct)} aria-valuemin={0} aria-valuemax={100}
         aria-label={label}>
      <span className="meter__fill" style={{ width: `${pct}%` }} />
    </div>
  );
}

/* ---- Feld ------------------------------------------------------------ */

export function Field({ label, hint, children }: {
  label: string; hint?: string; children: ReactNode;
}) {
  return (
    <label className="field">
      <span className="field__label">{label}</span>
      {children}
      {hint && <span className="field__hint">{hint}</span>}
    </label>
  );
}

/* ---- Schalter -------------------------------------------------------- */

export function Switch({ checked, onChange, label }: {
  checked: boolean; onChange: (v: boolean) => void; label: string;
}) {
  return (
    <button
      role="switch" aria-checked={checked} aria-label={label}
      className={`switch${checked ? " is-on" : ""}`}
      onClick={() => onChange(!checked)}
    >
      <span className="switch__knob" />
    </button>
  );
}
