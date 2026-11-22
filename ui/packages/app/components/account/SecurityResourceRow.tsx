import type { ReactNode } from "react";
import { ResourceRow } from "@agentsfleet/design-system";

type Props = {
  kind: "key" | "session";
  title: string;
  icon: ReactNode;
  detail: ReactNode;
  badge?: ReactNode;
  lastUsed?: ReactNode;
  action: ReactNode;
};

export function SecurityResourceRow({kind, title, icon, detail, badge, lastUsed, action}: Props) {
  const session = kind === "session";
  return (
    <ResourceRow as={session ? "div" : "li"} density={session ? "compact" : "default"} icon={icon} actions={<>
      {lastUsed && <span className="text-eyebrow text-text-dim">{lastUsed}</span>}
      {action}
    </>}>
      <div className="security-resource-title"><span>{title}</span>{badge}</div>
      <p className={session ? "mt-1.5 text-label text-muted-foreground" : "mt-sm text-label text-muted-foreground"}>{detail}</p>
    </ResourceRow>
  );
}
