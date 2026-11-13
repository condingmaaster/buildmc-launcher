import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./app/App";
import "./styles/global.css";
import "./components/ui.css";

/* Das Standard-Kontextmenue der Webansicht verraet sofort, dass hier ein
   Browser laeuft -- in einer ausgelieferten Anwendung gehoert es weg.
   Waehrend der Entwicklung bleibt es erreichbar, damit "Untersuchen"
   weiter funktioniert. Eingabefelder behalten es immer: Ausschneiden,
   Kopieren und Einfuegen erwartet man dort. */
if (!import.meta.env.DEV) {
  document.addEventListener("contextmenu", (e) => {
    const t = e.target as HTMLElement;
    if (t.closest("input, textarea, [contenteditable]")) return;
    e.preventDefault();
  });
}

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
