import { cn } from "@agentsfleet/design-system";
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

// Tokens · wall · cost for the latest run (§3). Every figure is a server field
// off the event row — the strip does no token→cost arithmetic (Invariant 1);
// `cost_nanos` is the summed telemetry credit, and a run with no telemetry
// renders cost as "—", never a fabricated zero.
export default function RunMetricsStrip({ latest }: { latest: EventRow | null }) {
  if (latest === null) {
    return (
      <div className="rounded-md border border-border bg-card px-4 py-2" aria-label={METRICS_STRIP_LABEL}>
        <p className="text-sm text-muted-foreground">{METRICS_EMPTY}</p>
      </div>
    );
  }
  return (
    <dl
      aria-label={METRICS_STRIP_LABEL}
      className="flex flex-wrap items-center gap-lg rounded-md border border-border bg-card px-4 py-2"
    >
      <Metric label={METRICS_TOKENS_LABEL} value={latest.tokens === null ? METRICS_COST_UNKNOWN : latest.tokens.toLocaleString()} />
      <Metric label={METRICS_WALL_LABEL} value={latest.wall_ms === null ? METRICS_COST_UNKNOWN : formatMs(latest.wall_ms)} />
      <Metric
        label={METRICS_COST_LABEL}
        value={latest.cost_nanos === null ? METRICS_COST_UNKNOWN : formatDollars(latest.cost_nanos)}
        emphatic
      />
    </dl>
  );
}

function Metric({ label, value, emphatic }: { label: string; value: string; emphatic?: boolean }) {
  return (
    <div className="flex items-baseline gap-xs">
      <dt className="font-mono text-eyebrow uppercase text-muted-foreground">{label}</dt>
      <dd className={cn("font-mono text-sm tabular-nums", emphatic ? "text-foreground" : "text-muted-foreground")}>
        {value}
      </dd>
    </div>
  );
}
