"use client";

import {
  Badge,
  DescriptionList,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Time,
} from "@agentsfleet/design-system";
import { type RunnerLease } from "@/lib/api/runners";
import { LEASE_OUTCOME } from "@/lib/api/runners-types";
import { failureSentenceFor } from "@/lib/events/event-summary";
import {
  EXPIRED_ROW_DETAIL,
  EXPIRED_ROW_SENTENCE,
  OUTCOME_LABELS,
  REVIEW_EXPIRES_LABEL,
  REVIEW_LEASE_TITLE,
  REVIEW_MODEL_LABEL,
  REVIEW_OUTCOME_LABEL,
  REVIEW_PROVIDER_LABEL,
  UNKNOWN_OUTCOME_SENTENCE,
} from "./runner-copy";
import { agentDisplayName } from "@/lib/fleets/agent-label";

import { LeaseFleetDestination } from "./LeaseFleetDestination";
import { LeaseDiagnostics, LeaseFact } from "./LeaseDiagnostics";
import { modelLabel } from "@/lib/models/display";
import { providerLabel } from "@/lib/api/model-library-types";

// Per-lease panel: every field is a column on runner.leases or its
// joined Fleet event. The request payload is deliberately absent — under any
// outcome — and nothing here is a secret; credentials resolve per lease and
// are never stored on the row.
export function ReviewLease({
  lease,
  onOpenChange,
}: {
  lease: RunnerLease | null;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={lease !== null} onOpenChange={onOpenChange}>
      <DialogContent>
        {lease ? (
          <>
            <DialogHeader>
              <DialogTitle>{REVIEW_LEASE_TITLE}</DialogTitle>
              <DialogDescription className="break-words font-sans">
                {agentDisplayName(lease.fleet_id)} · {lease.event_type}
              </DialogDescription>
            </DialogHeader>
            <DescriptionList className="font-sans text-body-sm tabular-nums">
              <LeaseFact label={REVIEW_OUTCOME_LABEL}><OutcomeSummary lease={lease} /></LeaseFact>
              <LeaseFact label={REVIEW_PROVIDER_LABEL}>{providerLabel(lease.provider)}</LeaseFact>
              <LeaseFact label={REVIEW_MODEL_LABEL}><span title={lease.model}>{modelLabel(lease.model)}</span></LeaseFact>
              <LeaseFact label={REVIEW_EXPIRES_LABEL}><Time value={new Date(lease.lease_expires_at)} format="relative" /></LeaseFact>
            </DescriptionList>
            <LeaseFleetDestination key={`${lease.workspace_id}:${lease.fleet_id}`} workspaceId={lease.workspace_id} fleetId={lease.fleet_id} />
            <LeaseDiagnostics lease={lease} />
          </>
        ) : null}
      </DialogContent>
    </Dialog>
  );
}

function OutcomeSummary({ lease }: { lease: RunnerLease }) {
  if (lease.outcome === LEASE_OUTCOME.failed && lease.failure_label) {
    return (
      <span>
        <Badge variant="destructive">{OUTCOME_LABELS[lease.outcome]}</Badge>{" "}
        {failureSentenceFor(lease.failure_label)}
        {lease.failure_detail ? (
          <span className="mt-xs block text-label text-text-subtle">{lease.failure_detail}</span>
        ) : null}
      </span>
    );
  }
  if (lease.outcome === LEASE_OUTCOME.expired) {
    return (
      <span>
        <Badge variant="amber">{OUTCOME_LABELS[lease.outcome]}</Badge> {EXPIRED_ROW_SENTENCE}
        <span className="mt-xs block text-label text-text-subtle">{EXPIRED_ROW_DETAIL}</span>
      </span>
    );
  }
  if (lease.outcome === LEASE_OUTCOME.unknown) {
    return (
      <span>
        <Badge>{OUTCOME_LABELS[lease.outcome]}</Badge> {UNKNOWN_OUTCOME_SENTENCE}
      </span>
    );
  }
  return (
    <Badge variant={lease.outcome === LEASE_OUTCOME.succeeded ? "green" : "cyan"}>
      {OUTCOME_LABELS[lease.outcome]}
    </Badge>
  );
}
