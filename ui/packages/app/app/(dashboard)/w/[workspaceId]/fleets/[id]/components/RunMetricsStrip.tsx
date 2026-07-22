import type { ReactNode } from "react";
import {
  Badge,
  Button,
  Card,
  Time,
  cn,
  DescriptionDetails,
  DescriptionList,
  DescriptionTerm,
} from "@agentsfleet/design-system";
import Link from "next/link";
import type { EventRow } from "@/lib/api/events";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { formatMs } from "@/lib/utils";
import { outcomeFor } from "@/lib/events/event-summary";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import {
  METRICS_APPROVAL_LABEL,
  METRICS_APPROVALS_LABEL,
  METRICS_APPROVALS_UNAVAILABLE,
  METRICS_COST_LABEL,
  METRICS_OUTCOME_LABEL,
  METRICS_STATUS_LABEL,
  METRICS_VALUE_UNKNOWN,
  METRICS_EMPTY,
  METRICS_STRIP_LABEL,
  METRICS_UNAVAILABLE,
  METRICS_TOKENS_LABEL,
  METRICS_TIME_LABEL,
} from "./console-copy";

const COUNT_FORMATTER = new Intl.NumberFormat("en-US");
const OUTCOME_PREVIEW_CHARS = 120;

// Tokens · wall · cost for the latest run (§3). Every figure is a server field
// off the event row — the strip does no token→cost arithmetic (Invariant 1);
// `cost_nanos` is the summed telemetry credit, and a run with no telemetry
// renders cost as "—", never a fabricated zero.
export default function RunMetricsStrip({
  status,
  latest,
  pendingApprovals,
  pendingApprovalsHasMore,
  approvalsHref,
  summaryAvailable,
  approvalsAvailable,
}: {
  status: string;
  latest: EventRow | null;
  pendingApprovals: number;
  pendingApprovalsHasMore: boolean;
  approvalsHref: string;
  summaryAvailable: boolean;
  approvalsAvailable: boolean;
}) {
  return (
    <Card className="flex flex-col gap-lg p-lg xl:flex-row xl:items-center" aria-label={METRICS_STRIP_LABEL}>
      <DescriptionList layout="stacked" className="grid flex-1 grid-cols-2 gap-lg space-y-0 md:grid-cols-5">
        <Metric label={METRICS_STATUS_LABEL} value={status} status />
        <Metric
          label={METRICS_OUTCOME_LABEL}
          value={latestOutcome(latest, summaryAvailable)}
          detail={outcomeTime(latest, summaryAvailable)}
        />
        <Metric label={METRICS_TOKENS_LABEL} value={formatTokens(latest, summaryAvailable)} />
        <Metric label={METRICS_COST_LABEL} value={formatCost(latest, summaryAvailable)} emphatic />
        <Metric label={METRICS_TIME_LABEL} value={formatDuration(latest, summaryAvailable)} />
      </DescriptionList>
      {!approvalsAvailable ? (
        <span className="font-mono text-xs text-destructive">
          {METRICS_APPROVALS_UNAVAILABLE}
        </span>
      ) : pendingApprovals > 0 ? (
        <Button asChild variant="outline" size="sm">
          <Link href={approvalsHref}>
            {pendingApprovals}{pendingApprovalsHasMore ? "+" : ""}{" "}
            {pendingApprovals === 1 ? METRICS_APPROVAL_LABEL : METRICS_APPROVALS_LABEL} →
          </Link>
        </Button>
      ) : null}
    </Card>
  );
}

function Metric({
  label,
  value,
  detail,
  emphatic,
  status,
}: {
  label: string;
  value: string;
  detail?: ReactNode;
  emphatic?: boolean;
  status?: boolean;
}) {
  return (
    <div className="min-w-0">
      <DescriptionTerm className="font-mono text-eyebrow uppercase">{label}</DescriptionTerm>
      <DescriptionDetails
        className={cn(
          "mt-xs truncate font-mono text-sm tabular-nums",
          emphatic ? "text-foreground" : "text-muted-foreground",
        )}
      >
        {status ? (
          <Badge variant={value === AGENTSFLEET_STATUS.ACTIVE ? "live" : "default"}>
            {value}
          </Badge>
        ) : value}
      </DescriptionDetails>
      {detail ? (
        <p className="mt-xs truncate font-mono text-label text-muted-foreground tabular-nums">
          {detail}
        </p>
      ) : null}
    </div>
  );
}

// A sentence, never a machine tag. The runner's failure classes read as plain
// English through the shared vocabulary, so this strip cannot say
// `startup_posture` where the events table says "Failed a startup safety check".
function latestOutcome(latest: EventRow | null, available: boolean): string {
  if (!available) return METRICS_UNAVAILABLE;
  if (latest === null) return METRICS_EMPTY;
  const response = latest.response_text?.replace(/\s+/g, " ").trim();
  if (response) return response.slice(0, OUTCOME_PREVIEW_CHARS);
  return outcomeFor(latest);
}

// When the latest outcome happened, beside the sentence saying what it was —
// the approved design carries both.
function outcomeTime(latest: EventRow | null, available: boolean): ReactNode {
  if (!available || latest === null) return null;
  const at = new Date(latest.created_at);
  if (!Number.isFinite(at.getTime())) return null;
  return <Time value={at} format="clock" />;
}

function formatTokens(latest: EventRow | null, available: boolean): string {
  if (!available) return METRICS_VALUE_UNKNOWN;
  return latest?.tokens === null || latest?.tokens === undefined
    ? METRICS_VALUE_UNKNOWN
    : COUNT_FORMATTER.format(latest.tokens);
}

function formatDuration(latest: EventRow | null, available: boolean): string {
  if (!available) return METRICS_VALUE_UNKNOWN;
  return latest?.wall_ms === null || latest?.wall_ms === undefined
    ? METRICS_VALUE_UNKNOWN
    : formatMs(latest.wall_ms);
}

function formatCost(latest: EventRow | null, available: boolean): string {
  if (!available) return METRICS_VALUE_UNKNOWN;
  return latest?.cost_nanos === null || latest?.cost_nanos === undefined
    ? METRICS_VALUE_UNKNOWN
    : formatDollars(latest.cost_nanos);
}
