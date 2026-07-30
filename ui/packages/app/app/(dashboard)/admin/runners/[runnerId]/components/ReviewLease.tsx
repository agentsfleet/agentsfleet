"use client";

import Link from "next/link";
import {
  Badge,
  DescriptionDetails,
  DescriptionList,
  DescriptionTerm,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  Time,
} from "@agentsfleet/design-system";
import { LEASE_OUTCOME, type RunnerLease } from "@/lib/api/runners";
import { failureSentenceFor } from "@/lib/events/event-summary";
import { workspacePath } from "@/lib/workspace-routes";
import {
  EXPIRED_ROW_DETAIL,
  EXPIRED_ROW_SENTENCE,
  OPEN_FLEET_LABEL,
  OUTCOME_LABELS,
  REVIEW_EVENT_LABEL,
  REVIEW_EXPIRES_LABEL,
  REVIEW_FENCING_LABEL,
  REVIEW_KIND_LABEL,
  REVIEW_LEASE_ID_LABEL,
  REVIEW_LEASE_TITLE,
  REVIEW_MODEL_LABEL,
  REVIEW_OUTCOME_LABEL,
  REVIEW_POSTURE_LABEL,
  REVIEW_PROVIDER_LABEL,
  REVIEW_TOKENS_LABEL,
  UNKNOWN_OUTCOME_SENTENCE,
} from "./runner-copy";

const COUNT_FORMAT = new Intl.NumberFormat("en-US");
const METER_SEPARATOR = " · ";

// Per-lease panel: every field is a column on fleet.runner_leases or its
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
              <DialogDescription className="font-mono">
                {lease.fleet_name ?? lease.fleet_id} · {lease.event_type}
              </DialogDescription>
            </DialogHeader>
            <DescriptionList className="font-mono text-body-sm tabular-nums">
              <DescriptionTerm>{REVIEW_OUTCOME_LABEL}</DescriptionTerm>
              <DescriptionDetails>
                <OutcomeSummary lease={lease} />
              </DescriptionDetails>
              <DescriptionTerm>{REVIEW_LEASE_ID_LABEL}</DescriptionTerm>
              <DescriptionDetails className="break-all">{lease.id}</DescriptionDetails>
              <DescriptionTerm>{REVIEW_KIND_LABEL}</DescriptionTerm>
              <DescriptionDetails>{lease.kind}</DescriptionDetails>
              <DescriptionTerm>{REVIEW_FENCING_LABEL}</DescriptionTerm>
              <DescriptionDetails>{COUNT_FORMAT.format(lease.fencing_token)}</DescriptionDetails>
              <DescriptionTerm>{REVIEW_EXPIRES_LABEL}</DescriptionTerm>
              <DescriptionDetails>
                <Time value={new Date(lease.lease_expires_at)} format="relative" />
              </DescriptionDetails>
              <DescriptionTerm>{REVIEW_PROVIDER_LABEL}</DescriptionTerm>
              <DescriptionDetails>{lease.provider}</DescriptionDetails>
              <DescriptionTerm>{REVIEW_MODEL_LABEL}</DescriptionTerm>
              <DescriptionDetails>{lease.model}</DescriptionDetails>
              <DescriptionTerm>{REVIEW_POSTURE_LABEL}</DescriptionTerm>
              <DescriptionDetails>{lease.posture}</DescriptionDetails>
              <DescriptionTerm>{REVIEW_TOKENS_LABEL}</DescriptionTerm>
              <DescriptionDetails>
                {COUNT_FORMAT.format(lease.metered_input_tokens)} in{METER_SEPARATOR}
                {COUNT_FORMAT.format(lease.metered_cached_tokens)} cached{METER_SEPARATOR}
                {COUNT_FORMAT.format(lease.metered_output_tokens)} out
              </DescriptionDetails>
              <DescriptionTerm>{REVIEW_EVENT_LABEL}</DescriptionTerm>
              <DescriptionDetails className="break-all">
                {lease.event_id}
                {lease.fleet_name !== null ? (
                  <>
                    {" "}
                    <Link
                      className="whitespace-nowrap text-pulse no-underline"
                      href={workspacePath(lease.workspace_id, `fleets/${lease.fleet_id}`)}
                    >
                      {OPEN_FLEET_LABEL} →
                    </Link>
                  </>
                ) : null}
              </DescriptionDetails>
            </DescriptionList>
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
