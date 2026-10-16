"use client";

import type { Review } from "@/lib/api/draft-types";
import Link from "next/link";
import { Button, List, ListItem, StatusPill } from "@agentsfleet/design-system";
import { safeActionHref } from "./chat-defaults";

const CHECK_STATE = {ready: "ready", blocked: "blocked", missing: "missing"} as const;

export function SetupChecks({checks, label}: {checks: Review["readiness"]; label: string}) {
  return <List variant="plain" divided aria-label={label}>{checks.map((check) => {
    const href = safeActionHref(check.next_action?.href);
    const variant = check.state === CHECK_STATE.ready ? "success" : check.state === CHECK_STATE.blocked ? "danger"
      : check.state === CHECK_STATE.missing ? "warning" : "neutral";
    return <ListItem key={`${check.kind}:${check.subject}`}>
      <div className="flex flex-wrap items-center justify-between gap-sm">
        <div className="min-w-0 space-y-xs"><p className="text-sm font-medium">{check.subject}</p><p className="text-xs text-muted-foreground">{check.kind}</p></div>
        <StatusPill variant={variant}>{check.state}</StatusPill>
        {href ? <Button variant="link" asChild><Link href={href}>{check.next_action?.label ?? "Resolve"}</Link></Button> : null}
      </div>
    </ListItem>;
  })}</List>;
}
