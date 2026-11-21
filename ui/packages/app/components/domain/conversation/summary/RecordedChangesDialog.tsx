"use client";

import type { ReactElement } from "react";
import type { ChangeTotals, RecordedChange } from "./recorded-changes";
import { FileTextIcon } from "@phosphor-icons/react/ssr";
import { DashboardPanel, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle, DialogTrigger, List, ListItem } from "@agentsfleet/design-system";
import { groupRecordedChanges } from "./recorded-changes";

type Props = {changes: readonly RecordedChange[]; children: ReactElement};

/** Both entry points inspect the same recorded file facts. */
export function RecordedChangesDialog({changes, children}: Props) {
  const running = changes.some(change => change.running);
  const groups = groupRecordedChanges(changes);
  return <Dialog><DialogTrigger asChild>{children}</DialogTrigger>
    <DialogContent className="ph-no-capture ph-no-record max-h-[85dvh] overflow-y-auto">
      <DialogHeader><DialogTitle>Recorded changes</DialogTitle><DialogDescription>{running ? "Recorded writes, including work still in progress." : "Files with confirmed local writes."} Counts describe recorded edits; repository diffs and publication may differ.</DialogDescription></DialogHeader>
      <div className="space-y-lg">{groups.map(group => <DashboardPanel key={group.repository?.id ?? "unbound"} padding="none" className="overflow-hidden rounded-md bg-surface-deep">
        <div className="flex flex-wrap items-center gap-sm border-b border-border px-lg py-sm text-eyebrow text-muted-foreground">
          <span title={group.repository?.fullName}>{group.label} · {group.changes.length} {group.changes.length === 1 ? "file" : "files"}</span>
          <span className="ml-auto"><RecordedChangeCounts lines={group.lines} /></span>
        </div>
        <List variant="plain" className="space-y-md p-lg">{group.changes.map(change => <ListItem key={change.path} className="flex flex-wrap items-start gap-sm text-label">
          <FileTextIcon size={16} className="mt-xs shrink-0 text-muted-foreground" aria-hidden="true" />
          <span className="min-w-0 flex-1 break-all font-mono">{change.path}{change.running ? <span className="mt-xs block font-sans text-eyebrow text-muted-foreground">Write in progress</span> : null}</span>
          <RecordedChangeCounts lines={change.lines} />
        </ListItem>)}</List>
      </DashboardPanel>)}</div>
    </DialogContent>
  </Dialog>;
}

/** Unknown is never rendered as zero, and incomplete receipts remain visibly partial. */
export function RecordedChangeCounts({lines}: {lines: ChangeTotals}) {
  if (lines.kind === "unknown") return <span className="font-sans text-eyebrow text-muted-foreground">Line totals unavailable</span>;
  return <span className="inline-flex shrink-0 items-baseline gap-xs whitespace-nowrap font-mono text-eyebrow tabular-nums">
    {lines.kind === "partial" ? <span className="font-sans text-muted-foreground">At least</span> : null}
    {lines.added > 0 || lines.removed === 0 ? <span className="text-success">+{lines.added}</span> : null}
    {lines.removed > 0 ? <span className="text-destructive">−{lines.removed}</span> : null}
  </span>;
}
