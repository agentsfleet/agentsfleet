"use client";

import type { Review } from "@/lib/api/draft-types";
import Link from "next/link";
import { Button, List, ListItem, StatusPill } from "@agentsfleet/design-system";
import { safeActionHref } from "./chat-defaults";

const CHECK_STATE = {ready: "ready", blocked: "blocked", missing: "missing"} as const;
const CHECK_LABELS: Readonly<Record<string, string>> = {
  model: "Model availability", wallet: "Account balance", source: "Saved instructions", definition: "Saved instructions",
  policy: "Access rules", stream: "Run delivery", activation: "Fleet activation",
  credentials: "Connected account", credential: "Connected account", connections: "Integration connection", connection: "Integration connection",
  grants: "Permission", grant: "Permission",
};
const INTERNAL_KINDS = new Set(["model", "wallet", "source", "definition", "policy", "stream", "activation"]);

export function SetupChecks({checks, label}: {checks: Review["readiness"]; label: string}) {
  return <List variant="plain" divided aria-label={label}>{checks.map((check) => {
    const href = safeActionHref(check.next_action?.href);
    const variant = check.state === CHECK_STATE.ready ? "success" : check.state === CHECK_STATE.blocked ? "danger"
      : check.state === CHECK_STATE.missing ? "warning" : "neutral";
    return <ListItem key={`${check.kind}:${check.subject}`}>
      <div className="flex flex-wrap items-center justify-between gap-sm">
        <div className="min-w-0 space-y-xs"><p className="text-sm font-medium">{Object.hasOwn(CHECK_LABELS, check.kind) ? CHECK_LABELS[check.kind] : check.kind}</p>
          {!INTERNAL_KINDS.has(check.kind) ? <p className="break-words text-xs text-muted-foreground">{check.subject}</p> : null}</div>
        <StatusPill variant={variant}>{check.state}</StatusPill>
        {href ? <Button variant="link" asChild><Link href={href}>{check.next_action?.label ?? "Resolve"}</Link></Button> : null}
      </div>
    </ListItem>;
  })}</List>;
}
