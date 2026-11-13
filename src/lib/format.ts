/* Formatierung. Deutsche Konventionen, damit Zahlen und Zeiten lesbar sind. */

const nf = new Intl.NumberFormat("de-DE");

export const num = (n: number) => nf.format(Math.round(n));

export function bytes(n: number): string {
  if (!n) return "0 B";
  const u = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), u.length - 1);
  const v = n / 1024 ** i;
  return `${v.toFixed(v < 10 && i > 0 ? 1 : 0).replace(".", ",")} ${u[i]}`;
}

export function compact(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1).replace(".", ",")} Mio.`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(0)}k`;
  return nf.format(n);
}

/** Spielzeit als "12 h 30 min" -- die Einheit, in der Spieler denken. */
export function playtime(seconds: number): string {
  if (!seconds) return "nie gespielt";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  if (!h) return `${m} min`;
  return m ? `${num(h)} h ${m} min` : `${num(h)} h`;
}

const RELATIVE: [number, Intl.RelativeTimeFormatUnit][] = [
  [60, "second"], [3600, "minute"], [86400, "hour"],
  [604800, "day"], [2629800, "week"], [31557600, "month"],
];

const rtf = new Intl.RelativeTimeFormat("de-DE", { numeric: "auto" });

export function ago(iso: string): string {
  if (!iso) return "nie";
  const diff = (Date.parse(iso) - Date.now()) / 1000;
  const abs = Math.abs(diff);
  if (abs < 45) return "gerade eben";
  for (let i = 0; i < RELATIVE.length; i++) {
    const [limit, unit] = RELATIVE[i]!;
    if (abs < limit) {
      const div = i === 0 ? 1 : RELATIVE[i - 1]![0];
      return rtf.format(Math.round(diff / div), unit);
    }
  }
  return rtf.format(Math.round(diff / 31557600), "year");
}

export function duration(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  const pad = (x: number) => String(x).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}

export const LOADER_LABEL: Record<string, string> = {
  vanilla: "Vanilla", buildmc: "BuildMC", fabric: "Fabric",
  forge: "Forge", neoforge: "NeoForge", quilt: "Quilt",
};

/** Wandelt #rrggbb in die drei HSL-Kanaele der Tokens um. */
export function hexToHsl(hex: string): { h: number; s: number; l: number } | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex.trim());
  if (!m) return null;
  const v = parseInt(m[1]!, 16);
  const r = ((v >> 16) & 255) / 255, g = ((v >> 8) & 255) / 255, b = (v & 255) / 255;
  const max = Math.max(r, g, b), min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (!d) return { h: 0, s: 0, l: Math.round(l * 100) };
  const s = d / (1 - Math.abs(2 * l - 1));
  let h: number;
  if (max === r) h = ((g - b) / d) % 6;
  else if (max === g) h = (b - r) / d + 2;
  else h = (r - g) / d + 4;
  h = Math.round(h * 60);
  return { h: h < 0 ? h + 360 : h, s: Math.round(s * 100), l: Math.round(l * 100) };
}
