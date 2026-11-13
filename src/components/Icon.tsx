/* Eigene Strichzeichnungen, ein gemeinsames Raster (24px, Strich 1.7).
   Kein Icon-Paket als Abhaengigkeit -- das haelt den Strich einheitlich
   und das Bundle klein. */

export type IconName =
  | "library" | "compass" | "server" | "gear" | "play" | "stop"
  | "plus" | "search" | "download" | "trash" | "clone" | "folder"
  | "chip" | "warn" | "check" | "chevron" | "close" | "spark"
  | "clock" | "puzzle" | "camera" | "refresh" | "filter" | "grid" | "list" | "shield"
  | "grass" | "bedrock" | "rust" | "hybrid";

const P: Record<IconName, string> = {
  library: "M4 5.5A1.5 1.5 0 0 1 5.5 4h13A1.5 1.5 0 0 1 20 5.5V10H4zM4 14h16v4.5a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 18.5z",
  compass: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18M15.2 8.8l-1.9 4.5-4.5 1.9 1.9-4.5z",
  server:  "M4 6.5A1.5 1.5 0 0 1 5.5 5h13A1.5 1.5 0 0 1 20 6.5v3A1.5 1.5 0 0 1 18.5 11h-13A1.5 1.5 0 0 1 4 9.5zM4 14.5A1.5 1.5 0 0 1 5.5 13h13a1.5 1.5 0 0 1 1.5 1.5v3a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 4 17.5zM7.5 8h.01M7.5 16h.01",
  gear:    "M12 15a3 3 0 1 0 0-6 3 3 0 0 0 0 6M19.4 15a1.6 1.6 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.6 1.6 0 0 0-1.8-.3 1.6 1.6 0 0 0-1 1.5v.2a2 2 0 1 1-4 0v-.1a1.6 1.6 0 0 0-1-1.5 1.6 1.6 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.6 1.6 0 0 0 .3-1.8 1.6 1.6 0 0 0-1.5-1H2a2 2 0 1 1 0-4h.1a1.6 1.6 0 0 0 1.5-1 1.6 1.6 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.6 1.6 0 0 0 1.8.3H8a1.6 1.6 0 0 0 1-1.5V2a2 2 0 1 1 4 0v.1a1.6 1.6 0 0 0 1 1.5 1.6 1.6 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.6 1.6 0 0 0-.3 1.8V8a1.6 1.6 0 0 0 1.5 1h.2a2 2 0 1 1 0 4h-.1a1.6 1.6 0 0 0-1.5 1",
  play:    "M7 4.8v14.4a.8.8 0 0 0 1.2.7l11.2-7.2a.8.8 0 0 0 0-1.4L8.2 4.1A.8.8 0 0 0 7 4.8",
  stop:    "M6.5 6h11A1.5 1.5 0 0 1 19 7.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 16.5v-9A1.5 1.5 0 0 1 6.5 6",
  plus:    "M12 5v14M5 12h14",
  search:  "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14M20 20l-4.1-4.1",
  download:"M12 3v12M7.5 10.5 12 15l4.5-4.5M4 19h16",
  trash:   "M4 7h16M9.5 7V5.5A1.5 1.5 0 0 1 11 4h2a1.5 1.5 0 0 1 1.5 1.5V7M6 7l.8 11.6A1.5 1.5 0 0 0 8.3 20h7.4a1.5 1.5 0 0 0 1.5-1.4L18 7",
  clone:   "M9 9h9a1.5 1.5 0 0 1 1.5 1.5v9A1.5 1.5 0 0 1 18 21H9a1.5 1.5 0 0 1-1.5-1.5v-9A1.5 1.5 0 0 1 9 9M4.5 15A1.5 1.5 0 0 1 3 13.5v-9A1.5 1.5 0 0 1 4.5 3h9A1.5 1.5 0 0 1 15 4.5",
  folder:  "M3 7.5A1.5 1.5 0 0 1 4.5 6h4l2 2.5h7A1.5 1.5 0 0 1 19 10v8a1.5 1.5 0 0 1-1.5 1.5h-13A1.5 1.5 0 0 1 3 18z",
  chip:    "M8 8h8v8H8zM9 3v3M15 3v3M9 18v3M15 18v3M3 9h3M3 15h3M18 9h3M18 15h3",
  warn:    "M12 4.5 2.8 19a1 1 0 0 0 .9 1.5h16.6a1 1 0 0 0 .9-1.5zM12 10v4M12 17.2h.01",
  check:   "M4.5 12.5 9.5 17.5 19.5 6.5",
  chevron: "m9 6 6 6-6 6",
  close:   "M6 6l12 12M18 6 6 18",
  spark:   "M12 3l2.1 5.6L20 10.5l-5.9 1.9L12 18l-2.1-5.6L4 10.5l5.9-1.9zM18.5 3.5l.7 1.8 1.8.7-1.8.7-.7 1.8-.7-1.8-1.8-.7 1.8-.7z",
  clock:   "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18M12 7v5.2l3.2 2",
  puzzle:  "M10 4.5a1.5 1.5 0 0 1 3 0c0 .9-.6 1.2-.6 1.5h3.6A1.5 1.5 0 0 1 17.5 7.5v3c-.3 0-.6-.6-1.5-.6a1.5 1.5 0 0 0 0 3c.9 0 1.2-.6 1.5-.6v3.6a1.5 1.5 0 0 1-1.5 1.5h-3c0-.3.6-.6.6-1.5a1.5 1.5 0 0 0-3 0c0 .9.6 1.2.6 1.5h-3.6A1.5 1.5 0 0 1 6.1 16.5V13c.3 0 .6.6 1.5.6a1.5 1.5 0 0 0 0-3c-.9 0-1.2.6-1.5.6V7.5A1.5 1.5 0 0 1 7.6 6h3c0-.3-.6-.6-.6-1.5",
  camera:  "M3 8.5A1.5 1.5 0 0 1 4.5 7h2.2l1.4-2h7.8l1.4 2h2.2A1.5 1.5 0 0 1 21 8.5v9a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5zM12 16a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7",
  refresh: "M20 5v5h-5M4 19v-5h5M19.2 9.5A7.5 7.5 0 0 0 6.3 7.1L4 9.5M4.8 14.5a7.5 7.5 0 0 0 12.9 2.4L20 14.5",
  filter:  "M3.5 5.5h17l-6.6 7.6v5.3l-3.8 2.1v-7.4z",
  grid:    "M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z",
  list:    "M4 6h16M4 12h16M4 18h16",
  shield:  "M12 3.5 5 6v5.2c0 5 3 8.6 7 9.8 4-1.2 7-4.8 7-9.8V6zM9 12l2 2 4-4.5",
  grass:   "M4 4h16v16H4zM4 9h16",
  bedrock: "M4 4h16v16H4zM8 8h2v2H8zM14 7h2v2h-2zM9 14h2v2H9zM15 15h2v2h-2z",
  hybrid: "M7 4h4v4H7zM13 4h4v4h-4zM7 10h10v10H7zM10 13h4v4h-4zM12 8v2",
  rust: "M12 2.5l1.6 2 2.5-.6.6 2.5 2.5.6-.6 2.5 2 1.6-2 1.6.6 2.5-2.5.6-.6 2.5-2.5-.6L12 21.5l-1.6-2-2.5.6-.6-2.5-2.5-.6.6-2.5-2-1.6 2-1.6-.6-2.5 2.5-.6.6-2.5 2.5.6zM9.5 8.5v7M9.5 8.5h3.2a1.8 1.8 0 010 3.6H9.5M12.4 12.1l2.1 3.4",
};

const FILLED = new Set<IconName>(["play", "stop", "library", "spark", "filter"]);

export function Icon({ name, size = 20, className }: {
  name: IconName; size?: number; className?: string;
}) {
  const filled = FILLED.has(name);
  return (
    <svg
      className={className}
      width={size} height={size} viewBox="0 0 24 24"
      fill={filled ? "currentColor" : "none"}
      stroke={filled ? "none" : "currentColor"}
      strokeWidth={1.7} strokeLinecap="round" strokeLinejoin="round"
      aria-hidden="true" focusable="false"
    >
      <path d={P[name]} />
    </svg>
  );
}
