/** Der Backend-Puffer ist die Quelle: keine Lücken oder Duplikate durch
 * verpasste Ereignisse, überlappende Abrufe oder einen Instanzwechsel. */
export function watchLog(
  read: () => Promise<string[]>,
  publish: (lines: string[]) => void,
  intervalMs = 500,
) {
  let disposed = false;
  let revision = 0;
  let pending = false;
  let previous: string[] | undefined;

  const refresh = async () => {
    if (disposed || pending) return;
    pending = true;
    const requestedRevision = revision;
    try {
      const next = await read();
      if (disposed || requestedRevision !== revision) return;
      if (!previous || next.length !== previous.length || next.some((line, i) => line !== previous![i])) {
        previous = next;
        publish(next);
      }
    } catch {
      // Vorhandene Zeilen behalten; beim nächsten Takt erneut versuchen.
    } finally {
      pending = false;
    }
  };

  void refresh();
  const timer = setInterval(() => void refresh(), intervalMs);
  return {
    clear: async (erase: () => Promise<void>) => {
      // Ein älterer Abruf darf geleerte Zeilen nicht wieder einsetzen.
      revision++;
      await erase();
      if (disposed) return;
      revision++;
      previous = [];
      publish([]);
      void refresh();
    },
    dispose: () => {
      disposed = true;
      clearInterval(timer);
    },
  };
}
