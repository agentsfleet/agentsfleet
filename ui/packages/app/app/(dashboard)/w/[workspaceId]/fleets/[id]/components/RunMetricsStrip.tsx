import {
  Card,
  cn,
  DescriptionDetails,
  DescriptionList,
  DescriptionTerm,
} from "@agentsfleet/design-system";
import type { EventRow } from "@/lib/api/events";
import { formatMs } from "@/lib/utils";
import { formatDollars } from "@/app/(dashboard)/settings/billing/lib/charges";
import {
  METRICS_COST_LABEL,
  METRICS_COST_UNKNOWN,
  METRICS_EMPTY,
  METRICS_STRIP_LABEL,
  METRICS_TOKENS_LABEL,
  METRICS_WALL_LABEL,
} from "./console-copy";

const COUNT_FORMATTER = new Intl.NumberFormat("en-US");

// Tokens · wall · cost for the latest run (§3). Every figure is a server field
// off the event row — the strip does no token→cost arithmetic (Invariant 1);
// `cost_nanos` is the summed telemetry credit, and a run with no telemetry
// renders cost as "—", never a fabricated zero.
export default function RunMetricsStrip({ latest }: { latest: EventRow | null }) {
  if (latest === null) {
    return (
      <Card className="px-4 py-2" aria-label={METRICS_STRIP_LABEL}>
        <p className="text-sm text-muted-foreground">{METRICS_EMPTY}</p>
      </Card>
    );
  }
  return (
    <Card className="px-4 py-2" aria-label={METRICS_STRIP_LABEL}>
      <DescriptionList layout="stacked" className="flex flex-wrap items-center gap-lg space-y-0">
        <Metric label={METRICS_TOKENS_LABEL} value={latest.tokens === null ? METRICS_COST_UNKNOWN : COUNT_FORMATTER.format(latest.tokens)} />
        <Metric label={METRICS_WALL_LABEL} value={latest.wall_ms === null ? METRICS_COST_UNKNOWN : formatMs(latest.wall_ms)} />
        <Metric
          label={METRICS_COST_LABEL}
          value={latest.cost_nanos === null ? METRICS_COST_UNKNOWN : formatDollars(latest.cost_nanos)}
          emphatic
        />
      </DescriptionList>
    </Card>
  );
}

function Metric({ label, value, emphatic }: { label: string; value: string; emphatic?: boolean }) {
  return (
    <div className="flex items-baseline gap-xs">
      <DescriptionTerm className="font-mono text-eyebrow uppercase">{label}</DescriptionTerm>
      <DescriptionDetails className={cn("font-mono text-sm tabular-nums", emphatic ? "text-foreground" : "text-muted-foreground")}>
        {value}
      </DescriptionDetails>
    </div>
  );
}
