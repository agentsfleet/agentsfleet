import Link from "next/link";
import { ArrowRightIcon } from "lucide-react";
import { Badge, Button, Time, WakePulse, type BadgeVariant, type DataTableColumn } from "@agentsfleet/design-system";
import type { RunnerLease } from "@/lib/api/runners";
import { LEASE_DESTINATION, LEASE_OUTCOME } from "@/lib/api/runners-types";
import { failureSentenceFor } from "@/lib/events/event-summary";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { TIME_LABEL } from "@/lib/time-labels";
import { formatMs } from "@/lib/utils";
import { DEFAULT_WORKSPACE_SUBPATH, workspacePath } from "@/lib/workspace-routes";
import { shortWorkspaceId } from "./lease-filter-query";
import { EXPIRED_ROW_DETAIL, EXPIRED_ROW_SENTENCE, OUTCOME_LABELS, UNKNOWN_OUTCOME_SENTENCE,
  WORKSPACE_LABEL, VIEW_LEASE_LABEL, LEASE_EVENT_UNAVAILABLE } from "./runner-copy";

const VALUE_UNKNOWN = "—";
const SETTLED_OUTCOME_VARIANT: Partial<Record<RunnerLease["outcome"], BadgeVariant>> = {
  [LEASE_OUTCOME.succeeded]: "green", [LEASE_OUTCOME.failed]: "destructive", [LEASE_OUTCOME.expired]: "amber",
};

export function leaseColumns(onView: (lease: RunnerLease) => void): DataTableColumn<RunnerLease>[] {
  return [
    {key: "fleet", primary: true, header: "Fleet", cell: (lease) => <FleetCell lease={lease} onView={onView} />},
    {key: "workspace", header: WORKSPACE_LABEL, hideOnMobile: true, cell: (lease) => <WorkspaceCell lease={lease} />},
    {key: "work", header: "Work", cell: (lease) => <WorkCell lease={lease} />},
    {key: "time", header: TIME_LABEL.created, sortValue: (lease) => Date.parse(lease.created_at),
      cell: (lease) => <Time value={lease.created_at} format="relative"
        className="font-sans text-body-sm leading-body-sm text-muted-foreground tabular-nums" />},
    {key: "duration", header: "Duration", numeric: true, hideOnMobile: true,
      cell: (lease) => <span className="font-mono text-mono leading-mono text-muted-foreground tabular-nums">
        {lease.wall_ms === null ? VALUE_UNKNOWN : formatMs(lease.wall_ms)}</span>},
    {key: "outcome", header: "Status", cell: (lease) => <OutcomeCell lease={lease} />},
  ];
}

function FleetCell({lease, onView}: {lease: RunnerLease; onView: (lease: RunnerLease) => void}) {
  return <span className="flex min-w-0 flex-col items-start gap-xs">
    <span className="truncate text-sm" title={lease.fleet_id}>{agentDisplayName(lease.fleet_id)}</span>
    <Button variant="ghost" size="sm" aria-label={`${VIEW_LEASE_LABEL} for ${agentDisplayName(lease.fleet_id)}`}
      onClick={() => onView(lease)}>{VIEW_LEASE_LABEL}<ArrowRightIcon size={14} aria-hidden="true" /></Button>
  </span>;
}

function WorkCell({lease}: {lease: RunnerLease}) {
  return <span className="block min-w-0">
    <span className="block truncate text-muted-foreground">{lease.event_type ?? LEASE_EVENT_UNAVAILABLE}</span>
    {lease.actor !== null ? <span className="block truncate text-label text-text-subtle">by {lease.actor}</span> : null}
  </span>;
}

function WorkspaceCell({ lease }: { lease: RunnerLease }) {
  if (lease.destination !== LEASE_DESTINATION.available) {
    return <span className="truncate font-mono text-mono leading-mono text-muted-foreground" title={lease.workspace_id}>
      {shortWorkspaceId(lease.workspace_id)}
    </span>;
  }
  return (
    <span className="flex min-w-0 items-center gap-md">
      <Link
        className="truncate font-mono text-mono leading-mono text-pulse no-underline"
        href={workspacePath(lease.workspace_id, DEFAULT_WORKSPACE_SUBPATH)}
        title={lease.workspace_id}
      >
        {shortWorkspaceId(lease.workspace_id)}
      </Link>
    </span>
  );
}

// Outcome speaks the operator's language: a live lease pulses, a failed lease
// reads the shared failure sentence with the daemon's detail line under it, an
// expired lease states that the runner stopped renewing and the work was
// re-leased, and a missing event reads as not recorded — never a fabricated
// success.
function OutcomeCell({ lease }: { lease: RunnerLease }) {
  if (lease.outcome === LEASE_OUTCOME.running) {
    return (
      <span className="inline-flex items-center gap-md text-body-sm uppercase tracking-eyebrow text-pulse">
        <WakePulse live className="inline-block size-2 rounded-full bg-current" aria-hidden="true" />
        {OUTCOME_LABELS[lease.outcome]}
      </span>
    );
  }
  const variant = SETTLED_OUTCOME_VARIANT[lease.outcome] ?? "default";
  return (
    <span className="block min-w-0">
      <span className="flex flex-wrap items-center gap-md">
        <Badge variant={variant}>{OUTCOME_LABELS[lease.outcome]}</Badge>
        {lease.outcome === LEASE_OUTCOME.failed && lease.failure_label ? (
          <span className="text-muted-foreground">{failureSentenceFor(lease.failure_label)}</span>
        ) : null}
        {lease.outcome === LEASE_OUTCOME.expired ? (
          <span className="text-muted-foreground">{EXPIRED_ROW_SENTENCE}</span>
        ) : null}
        {lease.outcome === LEASE_OUTCOME.unknown ? (
          <span className="text-muted-foreground">{UNKNOWN_OUTCOME_SENTENCE}</span>
        ) : null}
      </span>
      {lease.outcome === LEASE_OUTCOME.failed && lease.failure_detail ? (
        <span className="mt-xs block truncate text-label text-text-subtle">{lease.failure_detail}</span>
      ) : null}
      {lease.outcome === LEASE_OUTCOME.expired ? (
        <span className="mt-xs block truncate text-label text-text-subtle">{EXPIRED_ROW_DETAIL}</span>
      ) : null}
    </span>
  );
}
