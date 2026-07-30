import { cn, WakePulse } from "@agentsfleet/design-system";
import {
  RUNNER_ADMIN_STATE,
  type RunnerAdminState,
  type RunnerLiveness,
} from "@/lib/api/runners";

// The Fleets status treatment for a host: a dot plus uppercase mono text,
// administrative state before liveness — what the operator DECIDED, then what
// the host is DOING. Mint (and the wake ring) is reserved for a genuinely
// awake host: administratively active and actually heard from. A cordoned,
// draining or offline host gets a static dot, so mint stays a live signal.

const STATUS_SEPARATOR = " · ";

export function runnerIsAwake(adminState: RunnerAdminState, liveness: RunnerLiveness): boolean {
  return adminState === RUNNER_ADMIN_STATE.active && (liveness === "busy" || liveness === "online");
}

export function RunnerStatus({
  adminState,
  liveness,
  className,
}: {
  adminState: RunnerAdminState;
  liveness: RunnerLiveness;
  className?: string;
}) {
  const awake = runnerIsAwake(adminState, liveness);
  const offline = liveness === "offline";
  return (
    <span
      data-awake={awake ? "true" : undefined}
      className={cn(
        "inline-flex items-center gap-md font-mono text-body-sm uppercase tracking-eyebrow",
        awake ? "text-pulse" : offline ? "text-text-subtle" : "text-muted-foreground",
        className,
      )}
    >
      <WakePulse
        live={awake}
        className="inline-block size-2 shrink-0 rounded-full bg-current"
        aria-hidden="true"
      />
      {adminState}
      {STATUS_SEPARATOR}
      {liveness}
    </span>
  );
}
