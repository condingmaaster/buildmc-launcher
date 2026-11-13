import { useEffect, useState } from "react";

/* Spielerkopf aus der Skin-Grafik.
 *
 * Ein Minecraft-Skin ist 64x64 Bildpunkte gross und fest aufgeteilt: das
 * Gesicht liegt auf (8,8)-(16,16), die Hut-Ebene auf (40,8)-(48,16). Beide
 * werden per Hintergrundversatz ausgeschnitten und uebereinandergelegt --
 * dafuer braucht es weder Canvas noch einen Dienst, der Koepfe rendert.
 *
 * Ohne Skin tritt ein erzeugtes Ersatzbild an die Stelle: Farbton und
 * Muster leiten sich aus der UUID ab, sind also je Konto stabil.
 */

const SKIN_PX = 64;
const FACE = { x: 8, y: 8 };
const HAT = { x: 40, y: 8 };
const PART = 8; // Kantenlaenge des Gesichts in Skin-Punkten

/** Stabiler Farbton aus einer Zeichenkette. */
function hueOf(seed: string): number {
  let h = 0;
  for (let i = 0; i < seed.length; i++) {
    h = (h * 31 + seed.charCodeAt(i)) % 360;
  }
  return h;
}

export function Avatar({ name, uuid, skinUrl, size = 32 }: {
  name: string;
  uuid?: string;
  skinUrl?: string | null;
  size?: number;
}) {
  const label = name.slice(0, 1).toUpperCase();

  if (skinUrl) {
    const scale = size / PART;
    const layer = (p: { x: number; y: number }) => ({
      backgroundImage: `url(${skinUrl})`,
      backgroundSize: `${SKIN_PX * scale}px ${SKIN_PX * scale}px`,
      backgroundPosition: `-${p.x * scale}px -${p.y * scale}px`,
      imageRendering: "pixelated" as const,
    });

    return (
      <span className="avatar avatar--skin"
            style={{ width: size, height: size, ...layer(FACE) }}
            role="img" aria-label={`Kopf von ${name}`}>
        {/* Hut-Ebene daraufgelegt; viele Skins tragen dort Haare oder Brille. */}
        <span className="avatar__hat" style={layer(HAT)} aria-hidden="true" />
      </span>
    );
  }

  const hue = hueOf(uuid || name);
  return (
    <span
      className="avatar avatar--fallback"
      style={{ width: size, height: size, fontSize: size * 0.44,
               "--avatar-h": String(hue) } as React.CSSProperties}
      role="img" aria-label={name}
    >
      {label}
    </span>
  );
}

/* Symbol eines Projekts aus den Mod-Portalen.
 *
 * Modrinth und CurseForge liefern eine Bildadresse, aber nicht immer --
 * und sie kann ins Leere laufen. Deshalb wird ein Fehlschlag abgefangen
 * und auf dasselbe erzeugte Ersatzbild zurueckgegriffen.
 */
export function ProjectIcon({ title, url, size = 42 }: {
  title: string; url?: string | null; size?: number;
}) {
  /* Ein toter Verweis wird ueber den Zustand abgefangen, nicht durch
     Eingriff ins DOM -- sonst stellt das naechste Rendern ihn wieder her. */
  const [broken, setBroken] = useState(false);
  useEffect(() => { setBroken(false); }, [url]);

  const hue = hueOf(title);
  const style = { width: size, height: size,
                  "--avatar-h": String(hue) } as React.CSSProperties;

  if (!url || broken) {
    return (
      <span className="picon picon--fallback"
            style={{ ...style, fontSize: size * 0.4 }} aria-hidden="true">
        {title.slice(0, 1).toUpperCase()}
      </span>
    );
  }

  return (
    <img
      className="picon"
      style={style}
      src={url}
      alt=""
      loading="lazy"
      decoding="async"
      onError={() => setBroken(true)}
    />
  );
}
