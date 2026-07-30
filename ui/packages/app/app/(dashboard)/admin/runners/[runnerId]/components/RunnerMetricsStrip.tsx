import type { ReactNode } from "react";
import {
  Card,
  cn,
  DescriptionDetails,
  DescriptionList,
  DescriptionTerm,
  Time,
} from "@agentsfleet/design-system";
import type { RunnerDetail } from "@/lib/api/runners";
import {
  STRIP_ACQUIRED_LABEL,
  STRIP_EXPIRED_DETAIL,
  STRIP_EXPIRED_LABEL,
  STRIP_FAILED_DETAIL,
  STRIP_FAILED_LABEL,
  STRIP_HEARTBEAT_LABEL,
  STRIP_LABEL,
  STRIP_LEASES_NOW_LABEL,
  STRIP_LIFETIME_DETAIL,
  STRIP_SUCCEEDED_LABEL,
  STRIP_VALUE_UNKNOWN,
} from "./runner-copy";

const COUNT_FORMAT = new Intl.NumberFormat("en-US");
const FLEETS_DETAIL_SINGULAR = "Fleet";
const FLEETS_DETAIL_PLURAL = "Fleets";

// The six-cell strip mirroring RunMetricsStrip's shape: uppercase mono label
// over tabular value over a quieter detail line, left-border dividers. Every
// figure is a durable-state field off the single-runner read — the strip does
// no arithmetic and renders no percentage, ratio or capacity figure. Outcome
// counters carry their status colour, the same tokens the row badges use.
export default function RunnerMetricsStrip({ runner }: { runner: RunnerDetail }) {
  const fleets_noun = runner.active_fleet_count === 1 ? FLEETS_DETAIL_SINGULAR : FLEETS_DETAIL_PLURAL;
  return (
    <Card className="p-lg" aria-label={STRIP_LABEL}>
      <DescriptionList layout="stacked" className="grid grid-cols-2 gap-lg space-y-0 md:grid-cols-6">
        <Metric
          label={STRIP_HEARTBEAT_LABEL}
          value={
            runner.last_seen_at > 0 ? (
              <Time value={new Date(runner.last_seen_at)} format="relative" tooltip={false} />
            ) : (
              STRIP_VALUE_UNKNOWN
            )
          }
          detail={
            runner.last_seen_at > 0 ? <Time value={new Date(runner.last_seen_at)} format="clock" /> : null
          }
        />
        <Metric
          label={STRIP_LEASES_NOW_LABEL}
          value={COUNT_FORMAT.format(runner.active_lease_count)}
          detail={`across ${runner.active_fleet_count} ${fleets_noun}`}
          divided
        />
        <Metric
          label={STRIP_ACQUIRED_LABEL}
          value={COUNT_FORMAT.format(runner.leases_acquired)}
          detail={STRIP_LIFETIME_DETAIL}
          divided
        />
        <Metric
          label={STRIP_SUCCEEDED_LABEL}
          value={COUNT_FORMAT.format(runner.leases_succeeded)}
          tone="text-success"
          divided
        />
        <Metric
          label={STRIP_FAILED_LABEL}
          value={COUNT_FORMAT.format(runner.leases_failed)}
          detail={STRIP_FAILED_DETAIL}
          tone="text-error"
          divided
        />
        <Metric
          label={STRIP_EXPIRED_LABEL}
          value={COUNT_FORMAT.format(runner.leases_expired)}
          detail={STRIP_EXPIRED_DETAIL}
          tone="text-warn"
          divided
        />
      </DescriptionList>
    </Card>
  );
}

function Metric({
  label,
  value,
  detail,
  tone,
  divided,
}: {
  label: string;
  value: ReactNode;
  detail?: ReactNode;
  tone?: string;
  divided?: boolean;
}) {
  return (
    <div className={cn("min-w-0", divided && "md:border-l md:border-border md:pl-lg")}>
      <DescriptionTerm className="font-mono text-eyebrow uppercase">{label}</DescriptionTerm>
      <DescriptionDetails
        className={cn("mt-xs truncate font-mono text-sm tabular-nums", tone ?? "text-foreground")}
      >
        {value}
      </DescriptionDetails>
      {detail ? (
        <p className="mt-xs truncate font-mono text-label text-muted-foreground tabular-nums">{detail}</p>
      ) : null}
    </div>
  );
}
