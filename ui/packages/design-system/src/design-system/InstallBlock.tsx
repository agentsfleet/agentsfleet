import { Button } from "./Button";
import Terminal from "./Terminal";
import { cn } from "../utils";

type Action = {
  label: string;
  to: string;
  variant?: "default" | "ghost" | "outline" | "double-border";
  external?: boolean;
};

type Props = {
  title: string;
  command: string;
  actions: Action[];
  className?: string;
};

/*
 * InstallBlock renders plain <a> action children so the package stays
 * router-agnostic (RSC-safe, no react-router-dom / next/link import).
 * Consumers wanting router-aware navigation compose Button + <Link>
 * directly at the call-site instead of using actions[].
 */
export default function InstallBlock({ title, command, actions, className }: Props) {
  return (
    <div
      className={cn(
        "rounded-md border border-border bg-card p-3xl",
        className,
      )}
    >
      <h2 className="mt-0 mb-lg text-2xl">{title}</h2>
      <Terminal label={`${title} command`} copyable>
        {command}
      </Terminal>
      <div className="mt-xl flex flex-wrap gap-md">
        {actions.map((a) => (
          <Button key={a.label} asChild variant={a.variant ?? "default"}>
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
