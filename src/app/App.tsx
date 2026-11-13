import { useEffect } from "react";
import { useApp } from "../lib/store";
import { Rail } from "./Rail";
import { Toasts } from "./Toasts";
import { DownloadBar } from "./DownloadBar";
import { Library } from "../views/Library";
import { InstanceDetail } from "../views/InstanceDetail";
import { Browse } from "../views/Browse";
import { Assistant } from "../views/Assistant";
import { Servers } from "../views/Servers";
import { Settings } from "../views/Settings";
import { Security } from "../views/Security";
import { Bedrock } from "../views/Bedrock";
import { RustEdition } from "../views/RustEdition";
import { Boot } from "./Boot";
import "./shell.css";

export function App() {
  const ready = useApp((s) => s.ready);
  const view = useApp((s) => s.view);
  const edition = useApp((s) => s.edition);
  const boot = useApp((s) => s.boot);

  useEffect(() => { void boot(); }, [boot]);

  if (!ready) return <Boot />;

  return (
    <div className="shell">
      <Rail />
      <main className="shell__main" key={edition + view.name + ("id" in view ? view.id : "")}>
        {view.name === "library"  && <Library />}
        {view.name === "instance" && <InstanceDetail id={view.id} />}
        {view.name === "browse"   && <Browse />}
        {view.name === "assistant" && <Assistant />}
        {view.name === "servers"  && <Servers />}
        {view.name === "settings" && <Settings />}
        {view.name === "security" && <Security />}
        {view.name === "bedrock" && <Bedrock />}
        {view.name === "rust" && <RustEdition variant="horizon" />}
        {view.name === "bhh" && <RustEdition variant="bhh" />}
      </main>
      <Toasts />
      <DownloadBar />
    </div>
  );
}
