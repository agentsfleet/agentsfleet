import { HeartPulseIcon } from "lucide-react";
import { cn, StatusLine, StatusLineItem, Time } from "@agentsfleet/design-system";
import type { RunnerDetail } from "@/lib/api/runners";
import {
  STRIP_FAILED_UNIT,
  STRIP_HEARTBEAT_LABEL,
  STRIP_LABEL,
  STRIP_LEASES_NOW_LABEL,
  STRIP_LIVE_UNIT,
  STRIP_OK_UNIT,
  STRIP_OUTCOMES_LABEL,
  STRIP_OUTCOMES_SEPARATOR,
  STRIP_VALUE_UNKNOWN,
} from "./runner-copy";

const COUNT_FORMAT = new Intl.NumberFormat("en-US");
const ICON_SIZE = 12;

// The line at the foot of the runner page: heartbeat · ok / failed · live.
// Every figure is a durable-state field off the single-runner read — the line
// does no arithmetic and renders no percentage, ratio or capacity figure.
// Outcome counters carry their status colour, the same tokens the row badges
// use. The full lifetime ledger (acquired, expired) lives on the Activity
// view; this line carries only what an operator glances at.
export default function RunnerStatusLine({
  runner,
  className,
}: {
  runner: RunnerDetail;
  className?: string;
}) {
  // last_seen_at = 0 is the never-connected sentinel: an honest dash, no
  // fabricated relative time.
  const seen = runner.last_seen_at > 0;
  return (
    <StatusLine
      aria-label={STRIP_LABEL}
      className={cn("border-t border-border bg-background py-md", className)}
    >
      <StatusLineItem tone={seen ? "pulse" : "neutral"}>
        <HeartPulseIcon size={ICON_SIZE} aria-hidden="true" className="shrink-0" />
        <span className="sr-only">{STRIP_HEARTBEAT_LABEL} </span>
        {seen ? (
          <Time value={new Date(runner.last_seen_at)} format="relative" tooltip={false} />
        ) : (
          STRIP_VALUE_UNKNOWN
        )}
      </StatusLineItem>
      <StatusLineItem>
        <span className="sr-only">{STRIP_OUTCOMES_LABEL} </span>
        <span className="text-success">
          {COUNT_FORMAT.format(runner.leases_succeeded)} {STRIP_OK_UNIT}
        </span>
        <span aria-hidden="true">{STRIP_OUTCOMES_SEPARATOR}</span>
        <span className="text-error">
          {COUNT_FORMAT.format(runner.leases_failed)} {STRIP_FAILED_UNIT}
        </span>
      </StatusLineItem>
      <StatusLineItem tone={runner.active_lease_count > 0 ? "foreground" : "neutral"}>
        <span className="sr-only">{STRIP_LEASES_NOW_LABEL} </span>
        {COUNT_FORMAT.format(runner.active_lease_count)} {STRIP_LIVE_UNIT}
      </StatusLineItem>
    </StatusLine>
  );
}
