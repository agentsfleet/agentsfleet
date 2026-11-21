import type { ReactNode } from "react";

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
  const Row = kind === "key" ? "li" : "div";
  return (
    <Row className="settings-resource-row" data-kind={kind}>
      <div className="settings-resource-copy security-resource-identity">
        <span className="settings-resource-icon" aria-hidden="true">{icon}</span>
        <div className="min-w-0">
          <div className="security-resource-title"><span>{title}</span>{badge}</div>
          <p className="security-resource-detail">{detail}</p>
        </div>
      </div>
      {lastUsed && <span className="text-eyebrow text-text-dim">{lastUsed}</span>}
      {action}
    </Row>
  );
}
