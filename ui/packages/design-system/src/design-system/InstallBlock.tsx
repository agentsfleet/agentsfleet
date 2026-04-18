import { Button } from "./Button";
import Terminal from "./Terminal";

type Action = {
  label: string;
  to: string;
  variant?: "primary" | "ghost" | "double-border";
  external?: boolean;
};

type Props = {
  title: string;
  command: string;
  actions: Action[];
};

/*
 * InstallBlock intentionally renders plain <a> children so the package stays
 * router-agnostic (RSC-safe, no react-router-dom / next/link import). Consumers
 * that want router-aware navigation can compose Button + their own Link element
 * directly at the call-site instead of using InstallBlock's actions array.
 */
export default function InstallBlock({ title, command, actions }: Props) {
  return (
    <div className="z-install-block">
      <h2>{title}</h2>
      <Terminal label={`${title} command`} copyable>
        {command}
      </Terminal>
      <div className="z-btn-row">
        {actions.map((a) => (
          <Button key={a.label} asChild variant={a.variant ?? "primary"}>
            <a
              href={a.to}
              target={a.external ? "_blank" : undefined}
              rel={a.external ? "noopener noreferrer" : undefined}
            >
              {a.label}
            </a>
          </Button>
        ))}
      </div>
    </div>
  );
}
