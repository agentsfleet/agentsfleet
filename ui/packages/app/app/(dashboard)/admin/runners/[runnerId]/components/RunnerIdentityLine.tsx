import { CircleHelpIcon } from "lucide-react";
import { Badge } from "@agentsfleet/design-system";
import { type CapabilityReport, type RunnerDetail } from "@/lib/api/runners";
import { SANDBOX_TIER_LABELS, type RunnerAdminState } from "@/lib/api/runners-types";
import { DEGRADED_BADGE_LABEL, RunnerStatus } from "../../components/RunnerStatus";
import { RunnerChecksBadge } from "./RunnerChecksBadge";
import { RUNNER_STATES_DOC_URL, RUNNER_STATES_HELP_LABEL } from "./runner-copy";

// The line under the header row: administrative state and liveness, the
// isolation tier, labels, the checks verdict (the full report opens from it),
// and — when a real verdict contradicts a real assignment — the mismatch,
// side by side with what the host reported.
//
// The help for the state words rides the words: a "Learn more" link sat
// between the status and the tier pills and split the row into text, link,
// pills, pill. As a question mark on the status itself the row reads status,
// then pills, and the help is attached to what it explains.
// `adminState` arrives separately from the runner because the header paints an
// action's target state before the server confirms it.

const HELP_ICON_SIZE = 14;
const ASSIGNMENT_UNMET_PREFIX = "assignment unmet: ";
const ACHIEVABLE_PREFIX = "host reports";
const MECHANISM_YES = "✓";
const MECHANISM_NO = "✗";

// The host's own report, rendered verbatim beside the assignment it failed —
// what the kernel can actually enforce, mechanism by mechanism. No derived
// "achievable tier": deriving one client-side would re-implement the server's
// reconciliation and drift from it.
function describeAchievable(cap: CapabilityReport): string {
  const controllers = cap.cgroup_controllers.length > 0 ? cap.cgroup_controllers.join(",") : MECHANISM_NO;
  return (
    `${ACHIEVABLE_PREFIX} landlock ${cap.landlock ? MECHANISM_YES : MECHANISM_NO}` +
    ` · seccomp ${cap.seccomp ? MECHANISM_YES : MECHANISM_NO}` +
    ` · cgroups ${controllers}` +
    ` · bubblewrap ${cap.bubblewrap ? MECHANISM_YES : MECHANISM_NO}` +
    ` · egress ${cap.egress_enforcement ? MECHANISM_YES : MECHANISM_NO}`
  );
}

export function RunnerIdentityLine({
  runner,
  adminState,
}: {
  runner: RunnerDetail;
  adminState: RunnerAdminState;
}) {
  return (
    <div className="flex flex-col gap-md">
      <div className="flex flex-wrap items-center gap-2xl text-body-sm text-muted-foreground">
        <span className="inline-flex items-center gap-sm">
          <RunnerStatus adminState={adminState} liveness={runner.liveness} />
          <a
            href={RUNNER_STATES_DOC_URL}
            target="_blank"
            rel="noopener noreferrer"
            aria-label={RUNNER_STATES_HELP_LABEL}
            title={RUNNER_STATES_HELP_LABEL}
            className="-m-sm inline-flex items-center p-sm text-muted-foreground transition-colors duration-snap ease-snap hover:text-pulse focus-visible:text-pulse"
          >
            <CircleHelpIcon size={HELP_ICON_SIZE} aria-hidden="true" />
          </a>
        </span>
        <span data-testid="runner-labels" className="inline-flex flex-wrap items-center gap-sm">
          <Badge>{SANDBOX_TIER_LABELS[runner.sandbox_tier]}</Badge>
          {runner.degraded ? <Badge variant="error">{DEGRADED_BADGE_LABEL}</Badge> : null}
          {runner.labels.map((label) => (
            <Badge key={label}>{label}</Badge>
          ))}
        </span>
        <RunnerChecksBadge runner={runner} />
      </div>
      {/* The mismatch line renders ONLY when a real verdict contradicts a real
          assignment: the reason names the specific missing mechanism, and the
          achievable line states what the host reported — assigned against
          achievable, side by side (Dimensions 4.1 / 4.2). */}
      {runner.degraded && runner.degraded_reason ? (
        <p className="font-sans text-body-sm text-destructive">
          {ASSIGNMENT_UNMET_PREFIX}
          {runner.degraded_reason}
          {runner.achievable ? ` · ${describeAchievable(runner.achievable)}` : ""}
        </p>
      ) : null}
    </div>
  );
}
