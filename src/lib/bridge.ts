/* =========================================================================
   Bruecke zum Rust-Backend.

   Laeuft die Oberflaeche in Tauri, geht jeder Aufruf per IPC nach Rust.
   Laeuft sie im nackten Browser (Vite-Dev, Gestaltungsarbeit, UI-Tests),
   bedient stattdessen ein Mock-Backend dieselbe Schnittstelle. Dadurch
   ist die Oberflaeche ohne Rust-Werkzeugkette entwickelbar und testbar --
   und es gibt genau eine Stelle, an der das entschieden wird.
   ========================================================================= */

import { mockInvoke, mockListen } from "./mock";

type Unlisten = () => void;

const inTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export const backend = inTauri ? "tauri" : ("mock" as const);

/** Ruft einen Tauri-Command auf. Wirft bei Fehlern eine BackendError. */
export async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  if (!inTauri) return mockInvoke<T>(cmd, args);
  const { invoke } = await import("@tauri-apps/api/core");
  try {
    return await invoke<T>(cmd, args);
  } catch (e) {
    throw new BackendError(cmd, e);
  }
}

/** Abonniert ein Backend-Ereignis. Gibt die Abmeldefunktion zurueck. */
export async function on<T>(event: string, handler: (payload: T) => void): Promise<Unlisten> {
  if (!inTauri) return mockListen<T>(event, handler);
  const { listen } = await import("@tauri-apps/api/event");
  return listen<T>(event, (e) => handler(e.payload));
}

export class BackendError extends Error {
  constructor(public readonly command: string, public readonly cause: unknown) {
    super(typeof cause === "string" ? cause : `Befehl "${command}" fehlgeschlagen`);
    this.name = "BackendError";
  }
}
