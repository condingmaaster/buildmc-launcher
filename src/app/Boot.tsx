/* Ladebild. Bewusst ruhig: es soll den Start nicht laenger wirken lassen,
   als er ist -- ein einzelner pulsierender Schein, kein Fortschrittstheater. */

export function Boot() {
  return (
    <div className="boot">
      <div className="boot__mark" aria-hidden="true">
        <svg viewBox="0 0 64 64" width="64" height="64">
          <defs>
            <linearGradient id="bm" x1="0" y1="0" x2="1" y2="1">
              <stop offset="0" stopColor="var(--accent-bright)" />
              <stop offset="1" stopColor="var(--accent-dim)" />
            </linearGradient>
          </defs>
          <path
            d="M12 20.5 32 9l20 11.5v23L32 55 12 43.5z"
            fill="url(#bm)" opacity="0.22"
          />
          <path
            d="M12 20.5 32 9l20 11.5v23L32 55 12 43.5zM32 32 12 20.5M32 32l20-11.5M32 32v23"
            fill="none" stroke="url(#bm)" strokeWidth="2.2"
            strokeLinejoin="round" strokeLinecap="round"
          />
        </svg>
      </div>
      <p className="boot__text">BuildMC wird geladen</p>
    </div>
  );
}
