"use client";

import type { Definition, Review } from "@/lib/api/draft-types";
import Link from "next/link";
import { CheckIcon, CircleIcon, CircleAlertIcon } from "lucide-react";
import { Button, List, ListItem } from "@agentsfleet/design-system";
import { safeActionHref } from "./chat-defaults";
import { SetupAccess } from "./SetupAccess";

const READY = "ready";
const BLOCKED = "blocked";
const CHECK_LABELS: Readonly<Record<string, string>> = {
  model: "Model", wallet: "Budget", source: "Instructions", definition: "Instructions",
  policy: "Access rules", stream: "Run delivery", activation: "Activation",
};
const CREDENTIAL_KINDS = new Set(["credentials", "credential", "connections", "connection"]);
const GRANT_KINDS = new Set(["grants", "grant"]);
const DEPENDENT_KINDS = new Set(["stream", "activation"]);

export function SetupChecks({checks, label, workspace, definition, onRefresh}: {checks: Review["readiness"]; label: string; workspace?: string; definition?: Definition; onRefresh?: () => Promise<void>}) {
  const visible = checks.filter(check => !DEPENDENT_KINDS.has(check.kind) || check.state === BLOCKED);
  return <div className="space-y-xs">
    <List variant="plain" className="flex flex-wrap gap-x-md gap-y-xs" aria-label={label}>{visible.map(check => {
      const ready = check.state === READY;
      const credential = CREDENTIAL_KINDS.has(check.kind);
      const grant = GRANT_KINDS.has(check.kind);
      const text = credential ? `${check.subject} access` : Object.hasOwn(CHECK_LABELS, check.kind) ? CHECK_LABELS[check.kind] : check.subject || check.kind;
      const Icon = ready ? CheckIcon : check.state === BLOCKED ? CircleAlertIcon : CircleIcon;
      const href = credential ? null : safeActionHref(check.next_action?.href);
      const status = <span className={`inline-flex items-center gap-xs text-label ${ready ? "text-muted-foreground" : "text-warning"}`} title={check.state}>
          <Icon className={`size-3.5 shrink-0 ${ready ? "text-success" : ""}`} aria-hidden="true" />
          <span>{text}{ready || grant ? "" : " needed"}</span><span className="sr-only">: {check.state}</span>
        </span>;
      return <ListItem key={`${check.kind}:${check.subject}`} className="flex items-center gap-xs p-0">
        {grant && href ? <Link href={href} className="inline-flex items-center hover:underline">{status}</Link> : status}
        {!grant && href ? <Button variant="link" size="sm" asChild><Link href={href}>{check.next_action?.label ?? "Resolve"}</Link></Button> : null}
      </ListItem>;
    })}</List>
    {workspace ? visible.filter(check => CREDENTIAL_KINDS.has(check.kind) && check.state !== READY).map(check =>
      <SetupAccess key={`${check.kind}:${check.subject}`} workspace={workspace} name={check.subject} definition={definition} onRefresh={onRefresh} />) : null}
  </div>;
}
