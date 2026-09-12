import type { ReactNode } from "react";
import Link from "next/link";
import {
  CheckIcon,
  CircleAlertIcon,
  CoinsIcon,
  HashIcon,
  HourglassIcon,
  LoaderCircleIcon,
  TimerIcon,
  type LucideIcon,
} from "lucide-react";
import { cn, StatusLine, StatusLineItem, type StatusLineTone, Time } from "@agentsfleet/design-system";
import type { RunFigures } from "@/lib/events/run-summary";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";
import { formatMs } from "@/lib/utils";
import { EVENT_STATUS, outcomeFor } from "@/lib/events/event-summary";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import {
  METRICS_APPROVAL_LABEL,
  METRICS_APPROVALS_LABEL,
  METRICS_COST_LABEL,
  METRICS_EMPTY,
  METRICS_OUTCOME_LABEL,
  METRICS_STATUS_LABEL,
  METRICS_STRIP_LABEL,
  METRICS_TIME_LABEL,
  METRICS_TOKENS_LABEL,
  METRICS_UNAVAILABLE,
  METRICS_VALUE_UNKNOWN,
} from "./console-copy";

const COUNT_FORMATTER = new Intl.NumberFormat("en-US");
const ICON_SIZE = 12;
const TOKENS_UNIT = "tok";
const APPROVALS_ARROW = "→";

type OutcomeCell = {
  text: string;
  tone: StatusLineTone;
  Icon: LucideIcon;
  /** True while the run is still going: the glyph turns. */
  live: boolean;
  at: Date | null;
};

// The line under the composer: status · latest outcome · tokens · spend ·
// duration, and the approvals link when any wait. Every figure is a server
// field off the event row — the line does no token→cost arithmetic;
// `cost_nanos` is the summed telemetry credit, and a run with no telemetry
// renders cost as "—", never a fabricated zero. The pending count is the
// server's own, off the fleet detail and then off the live tail's frames, so
// it is exact rather than a page length with a "+".
//
// Each cell carries its label visually hidden ahead of the figure. The glyph
// is what a sighted reader sees; the label is what a screen reader hears, and
// what the acceptance walk greps the line's text for.
export default function FleetStatusLine({
  status,
  latest,
  pendingApprovals,
  approvalsHref,
  summaryAvailable,
}: {
  status: string;
  latest: RunFigures | null;
  pendingApprovals: number;
  approvalsHref: string;
  summaryAvailable: boolean;
}) {
  const outcome = outcomeCell(latest, summaryAvailable);
  const OutcomeIcon = outcome.Icon;
  // No rule of its own: the card above already draws the edge, and a second
  // hairline 16px under it read as a doubled border.
  return (
    <StatusLine aria-label={METRICS_STRIP_LABEL}>
      <StatusLineItem
        tone={status === AGENTSFLEET_STATUS.ACTIVE ? "pulse" : "neutral"}
        className="uppercase"
      >
        <span className="size-2 shrink-0 rounded-full bg-current" aria-hidden="true" />
        <HiddenLabel>{METRICS_STATUS_LABEL}</HiddenLabel>
        {status}
      </StatusLineItem>
      <StatusLineItem tone={outcome.tone}>
        <OutcomeIcon
          size={ICON_SIZE}
          aria-hidden="true"
          className={cn("shrink-0", outcome.live && "animate-spin")}
        />
        <HiddenLabel>{METRICS_OUTCOME_LABEL}</HiddenLabel>
        <span className="truncate">{outcome.text}</span>
        {outcome.at ? (
          <Time value={outcome.at} format="clock" className="text-muted-foreground" />
        ) : null}
      </StatusLineItem>
      <Figure
        Icon={HashIcon}
        label={METRICS_TOKENS_LABEL}
        value={formatTokens(latest, summaryAvailable)}
        unit={TOKENS_UNIT}
      />
      <Figure
        Icon={CoinsIcon}
        label={METRICS_COST_LABEL}
        value={formatCost(latest, summaryAvailable)}
        tone="foreground"
      />
      <Figure Icon={TimerIcon} label={METRICS_TIME_LABEL} value={formatDuration(latest, summaryAvailable)} />
      {pendingApprovals > 0 ? (
        <StatusLineItem tone="warning">
          <Link href={approvalsHref} className="underline-offset-2 hover:underline">
            {pendingApprovals}{" "}
            {pendingApprovals === 1 ? METRICS_APPROVAL_LABEL : METRICS_APPROVALS_LABEL} {APPROVALS_ARROW}
          </Link>
        </StatusLineItem>
      ) : null}
    </StatusLine>
  );
}

function HiddenLabel({ children }: { children: ReactNode }) {
  return <span className="sr-only">{children} </span>;
}

function Figure({
  Icon,
  label,
  value,
  unit,
  tone,
}: {
  Icon: LucideIcon;
  label: string;
  value: string;
  unit?: string;
  tone?: StatusLineTone;
}) {
  return (
    <StatusLineItem tone={tone}>
      <Icon size={ICON_SIZE} aria-hidden="true" className="shrink-0" />
      <HiddenLabel>{label}</HiddenLabel>
      <span>{value}</span>
      {/* No unit beside a dash: "— tok" would read as a figure of nothing. */}
      {unit && value !== METRICS_VALUE_UNKNOWN ? (
        <span className="text-muted-foreground">{unit}</span>
      ) : null}
    </StatusLineItem>
  );
}

// A sentence, never a machine tag. The runner's failure classes read as plain
// English through the shared vocabulary, so this line cannot say
// `startup_posture` where the events table says "Failed a startup safety
// check". The list read carries no bodies, so the line states the outcome
// rather than quoting the answer — and, for a processed run, states only that
// it completed. The tone and glyph follow the same classification the
// sentence does, so a failure reads red before the words are read.
function outcomeCell(latest: RunFigures | null, available: boolean): OutcomeCell {
  if (!available) {
    return { text: METRICS_UNAVAILABLE, tone: "neutral", Icon: CircleAlertIcon, live: false, at: null };
  }
  if (latest === null) {
    return { text: METRICS_EMPTY, tone: "neutral", Icon: HourglassIcon, live: false, at: null };
  }
  // A row whose stored timestamp does not read as a date still renders its
  // outcome; the time is dropped instead of showing "Invalid Date".
  const stamp = new Date(latest.created_at);
  const at = Number.isFinite(stamp.getTime()) ? stamp : null;
  const text = outcomeFor(latest);
  if (latest.status === EVENT_STATUS.RECEIVED) {
    return { text, tone: "foreground", Icon: LoaderCircleIcon, live: true, at };
  }
  if (latest.failure_label || latest.status === EVENT_STATUS.FLEET_ERROR) {
    return { text, tone: "danger", Icon: CircleAlertIcon, live: false, at };
  }
  if (latest.status === EVENT_STATUS.GATE_BLOCKED) {
    return { text, tone: "warning", Icon: HourglassIcon, live: false, at };
  }
  return { text, tone: "success", Icon: CheckIcon, live: false, at };
}

function formatTokens(latest: RunFigures | null, available: boolean): string {
  if (!available) return METRICS_VALUE_UNKNOWN;
  return latest?.tokens === null || latest?.tokens === undefined
    ? METRICS_VALUE_UNKNOWN
    : COUNT_FORMATTER.format(latest.tokens);
}

function formatDuration(latest: RunFigures | null, available: boolean): string {
  if (!available) return METRICS_VALUE_UNKNOWN;
  return latest?.wall_ms === null || latest?.wall_ms === undefined
    ? METRICS_VALUE_UNKNOWN
    : formatMs(latest.wall_ms);
}

function formatCost(latest: RunFigures | null, available: boolean): string {
  if (!available) return METRICS_VALUE_UNKNOWN;
  return latest?.cost_nanos === null || latest?.cost_nanos === undefined
    ? METRICS_VALUE_UNKNOWN
    : formatDollars(latest.cost_nanos);
}
