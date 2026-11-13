import { editionHome, useApp, type Edition, type View } from "../lib/store";
import { Icon, type IconName } from "../components/Icon";
import { Avatar } from "../components/Avatar";

const JAVA_NAV: { name: View["name"]; icon: IconName; label: string }[] = [
  { name: "library",  icon: "library", label: "Bibliothek" },
  { name: "browse",   icon: "compass", label: "Entdecken" },
  { name: "assistant", icon: "spark", label: "Assistent" },
  { name: "servers",  icon: "server",  label: "Server" },
  { name: "security", icon: "shield",  label: "Sicherheit" },
  { name: "settings", icon: "gear",    label: "Einstellungen" },
];

const BEDROCK_NAV: { name: View["name"]; icon: IconName; label: string }[] = [
  { name: "bedrock",  icon: "bedrock", label: "Bibliothek" },
  { name: "settings", icon: "gear",    label: "Einstellungen" },
];

const RUST_NAV: { name: View["name"]; icon: IconName; label: string }[] = [
  { name: "rust",     icon: "rust",    label: "Bibliothek" },
  { name: "settings", icon: "gear",    label: "Einstellungen" },
];

const BHH_NAV: { name: View["name"]; icon: IconName; label: string }[] = [
  { name: "bhh",      icon: "hybrid",  label: "Bibliothek" },
  { name: "settings", icon: "gear",    label: "Einstellungen" },
];

const EDITION_INFO: Record<Edition, { label: string; icon: IconName }> = {
  java:    { label: "Java Edition", icon: "grass" },
  bedrock: { label: "Bedrock Edition", icon: "bedrock" },
  rust:    { label: "Horizon (Rust, inoffiziell)", icon: "rust" },
  bhh:     { label: "BuildMC Horizon Hybrid (Rust + Fabric, inoffiziell)", icon: "hybrid" },
};

/** Senkrechter Zwei-Wege-Schalter zwischen Java und Bedrock -- keine
 *  waagerechte Textkapsel: die Leiste ist nur ~84px breit und senkrecht
 *  aufgebaut, "Java | Bedrock" als Text passt darin nicht lesbar. */
function EditionSwitch() {
  const edition = useApp((s) => s.edition);
  const setEdition = useApp((s) => s.setEdition);
  const supported = useApp((s) => s.bedrockPlatformSupported);

  // Ohne mcpelauncher (Windows) bleiben Java und Rust.
  const editions: Edition[] = supported ? ["java", "bedrock", "rust", "bhh"] : ["java", "rust", "bhh"];
  const pos = Math.max(0, editions.indexOf(edition));

  return (
    <div className="ed" role="radiogroup" aria-label="Spielart" style={{ ["--ed-n" as string]: editions.length, ["--ed-i" as string]: pos }}>
      <span className="ed__thumb" aria-hidden="true" />
      {editions.map((e) => (
        <button
          key={e}
          role="radio"
          aria-checked={edition === e}
          aria-label={EDITION_INFO[e].label}
          title={EDITION_INFO[e].label}
          className={`ed__opt${edition === e ? " is-on" : ""}`}
          onClick={() => setEdition(e)}
        >
          <Icon name={EDITION_INFO[e].icon} size={17} />
        </button>
      ))}
    </div>
  );
}

export function Rail() {
  const view = useApp((s) => s.view);
  const go = useApp((s) => s.go);
  const accounts = useApp((s) => s.accounts);
  const running = useApp((s) => s.running);
  const edition = useApp((s) => s.edition);
  const bedrockRunning = useApp((s) => s.bedrockRunning);

  const active = accounts.find((a) => a.active);
  const rustRunning = useApp((s) => s.rustRunning);
  const nav = edition === "bedrock" ? BEDROCK_NAV : edition === "rust" ? RUST_NAV : edition === "bhh" ? BHH_NAV : JAVA_NAV;

  return (
    <nav className="rail" aria-label="Hauptbereiche">
      <button
        className="rail__mark"
        onClick={() => go(editionHome(edition))}
        aria-label="Zur Bibliothek"
      >
        <svg viewBox="0 0 64 64" width="30" height="30" aria-hidden="true">
          <path
            d="M12 20.5 32 9l20 11.5v23L32 55 12 43.5zM32 32 12 20.5M32 32l20-11.5M32 32v23"
            fill="none" stroke="currentColor" strokeWidth="3"
            strokeLinejoin="round" strokeLinecap="round"
          />
        </svg>
      </button>

      <EditionSwitch />

      <ul className="rail__nav" key={edition}>
        {nav.map((item) => {
          const on = view.name === item.name
            || (item.name === "library" && view.name === "instance");
          const dot = item.name === "library" && running.length > 0
            ? running.length
            : item.name === "bedrock" && bedrockRunning.length > 0
              ? bedrockRunning.length
              : (item.name === "rust" || item.name === "bhh") && rustRunning.length > 0
                ? rustRunning.length
                : 0;
          return (
            <li key={item.name}>
              <button
                className={`rail__btn${on ? " is-on" : ""}`}
                onClick={() => go({ name: item.name } as View)}
                aria-current={on ? "page" : undefined}
              >
                <Icon name={item.icon} size={21} />
                <span className="rail__label">{item.label}</span>
                {dot > 0 && <span className="rail__dot" aria-label={`${dot} laufend`} />}
              </button>
            </li>
          );
        })}
      </ul>

      {edition === "java" && (
        <button className="rail__account"
                title={active?.username ? `${active.username} — Konten verwalten`
                                        : "Anmelden"}
                onClick={() => go({ name: "settings" })}>
          <Avatar name={active?.username ?? "?"} uuid={active?.uuid}
                  skinUrl={active?.skin_url} size={30} />
          <span className="rail__label">{active?.username ?? "Anmelden"}</span>
        </button>
      )}
    </nav>
  );
}
