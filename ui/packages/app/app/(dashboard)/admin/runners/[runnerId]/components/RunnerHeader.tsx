"use client";

import { useState, useTransition } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { CircleHelpIcon, ExternalLinkIcon, RefreshCwIcon } from "lucide-react";
import { Alert, Badge, Button, CopyButton, IconAction, TooltipButton } from "@agentsfleet/design-system";
import {
  SANDBOX_TIER_LABELS,
  type CapabilityReport,
  type RunnerStateAction,
  type RunnerDetail,
  type RunnerListItem,
} from "@/lib/api/runners";
import EditPolicyDialogDynamic from "@/components/domain/island-dynamic/EditPolicyDialogDynamic";
import { runnersIndexPath } from "@/lib/runner-routes";
import { presentErrorString } from "@/lib/errors";
import {
  ACTION_CONFIG,
  DELETE_ACTION_CONFIG,
  SELFTEST_ACTION_CONFIG,
  actionsFor,
  canDelete,
  canSelftest,
} from "../../components/RunnerListCells";
import {
  RunnerActionConfirm,
  type RunnerActionConfirmTarget,
  type RunnerDeleteConfirmTarget,
} from "../../components/RunnerDialogs";
import { updateRunnerAdminStateAction, deleteRunnerAction, requestRunnerSelftestAction } from "../../actions";
import { DEGRADED_BADGE_LABEL, RunnerStatus } from "../../components/RunnerStatus";
import {
  COPY_RUNNER_ID_LABEL,
  OPEN_GRAFANA_LABEL,
  REFRESH_RUNNER_LABEL,
  RUNNER_ACTIONS_LABEL,
  RUNNER_BREADCRUMB_LABEL,
  RUNNER_STATES_DOC_URL,
  RUNNERS_CRUMB_LABEL,
} from "./runner-copy";

// The FleetHeader shape verbatim: breadcrumb left, actions right, one
// vertically-centred row, and NO second title — the breadcrumb already names
// the host and the page's <h1> is screen-reader-only. The runner id is the
// paste-into-a-ticket value, so it rides a CopyButton beside the breadcrumb
// rather than 26 characters of visible text. Identity — status, isolation
// tier, labels — is one line below; enrolment is not repeated here because
// Activity's registered record carries it with the real date.

const ASSIGNMENT_UNMET_PREFIX = "assignment unmet: ";
const SelftestIcon = SELFTEST_ACTION_CONFIG.icon;
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

export function RunnerHeader({
  runner,
  grafanaHref,
  canWrite,
}: {
  runner: RunnerDetail;
  grafanaHref: string | null;
  /** Whether this operator holds runner:write. Every mutating control is gated
   * on it — the server actions refuse without the scope regardless, so a button
   * a read-only operator can press is an error message pretending to be a
   * feature. */
  canWrite: boolean;
}) {
  const router = useRouter();
  const [confirmAction, setConfirmAction] = useState<RunnerActionConfirmTarget>(null);
  const [confirmDelete, setConfirmDelete] = useState<RunnerDeleteConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);
  // The self-test has its own error slot because it has no confirm dialog to
  // carry one: `error` renders only inside RunnerActionConfirm, which stays
  // closed for a self-test, so a shared slot would swallow the refusal.
  const [selftestError, setSelftestError] = useState<string | null>(null);
  const [, startTransition] = useTransition();

  function requestAction(action: RunnerStateAction) {
    setError(null);
    setConfirmAction({ runner, action, ...ACTION_CONFIG[action] });
  }

  function runAction(target: NonNullable<RunnerActionConfirmTarget>) {
    startTransition(async () => {
      const result = await updateRunnerAdminStateAction(runner.id, target.action);
      if (!result.ok) {
        // A concurrent transition answers 409 with the real state; refreshing
        // re-reads the header so the badge is never stale beside the error.
        setError(
          presentErrorString({ errorCode: result.errorCode, message: result.error, action: target.errorAction }),
        );
        router.refresh();
        return;
      }
      setConfirmAction(null);
      router.refresh();
    });
  }

  // No confirm step and no wait for a verdict: the request is recorded, the
  // page re-reads, and the pending state renders from `selftest_requested_at`.
  function runSelftest() {
    setSelftestError(null);
    startTransition(async () => {
      const result = await requestRunnerSelftestAction(runner.id);
      if (!result.ok) {
        setSelftestError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: SELFTEST_ACTION_CONFIG.errorAction,
          }),
        );
      }
      router.refresh();
    });
  }

  function runDelete(target: NonNullable<RunnerDeleteConfirmTarget>) {
    startTransition(async () => {
      const result = await deleteRunnerAction(runner.id);
      if (!result.ok) {
        setError(
          presentErrorString({ errorCode: result.errorCode, message: result.error, action: target.errorAction }),
        );
        router.refresh();
        return;
      }
      setConfirmDelete(null);
      router.push(runnersIndexPath());
    });
  }

  const runnerForDialog: RunnerListItem = runner;

  return (
    <div>
      <div className="mb-md flex flex-col gap-md sm:flex-row sm:items-center sm:justify-between">
        <h1 className="sr-only">{runner.host_id}</h1>
        <nav
          aria-label={RUNNER_BREADCRUMB_LABEL}
          className="flex shrink-0 items-center font-mono text-sm text-muted-foreground"
        >
          <Link href={runnersIndexPath()} className="hover:text-foreground">
            {RUNNERS_CRUMB_LABEL}
          </Link>
          <span aria-hidden="true" className="mx-md">/</span>
          <span className="text-foreground">{runner.host_id}</span>
          <CopyButton value={runner.id} label={COPY_RUNNER_ID_LABEL} className="ml-md" />
        </nav>
        <div aria-label={RUNNER_ACTIONS_LABEL} className="flex flex-wrap items-center justify-end gap-sm">
          {canWrite ? (
            <EditPolicyDialogDynamic
              runnerId={runner.id}
              current={runner.assigned_policy}
              onSaved={() => router.refresh()}
            />
          ) : null}
          {canWrite && canSelftest(runner.admin_state) ? (
            <Button
              variant="outline"
              size="sm"
              disabled={runner.selftest_requested_at !== null}
              onClick={runSelftest}
            >
              <SelftestIcon aria-hidden="true" />
              {runner.selftest_requested_at !== null
                ? SELFTEST_ACTION_CONFIG.pendingLabel
                : SELFTEST_ACTION_CONFIG.label}
            </Button>
          ) : null}
          {canWrite
            ? actionsFor(runner.admin_state).map((action) => {
                const config = ACTION_CONFIG[action];
                const ActionIcon = config.icon;
                // A not-yet-operable action renders disabled with its reason —
                // never a hidden control, never one that pretends to work. The
                // handler stays wired: the native disabled attribute is what
                // keeps the dialog closed and the PATCH unsent.
                // TooltipButton, not title=: a title attribute is mouse-only,
                // and a natively disabled button leaves keyboard and screen-
                // reader users two dead controls with no discoverable reason.
                // The primitive's span wrapper keeps hover working while
                // disabled, and the tooltip reads out as the reason.
                // One variant expression for both shapes: duplicating the
                // ternary inside the disabled arm leaves its destructive half
                // unreachable (only cordon and drain carry a reason, and both
                // are default-intent), which is a branch no test can honestly
                // cover.
                const variant = config.intent === "destructive" ? "destructive" : "outline";
                if (config.disabledReason !== undefined) {
                  return (
                    <TooltipButton
                      key={action}
                      variant={variant}
                      size="sm"
                      disabled
                      tooltip={config.disabledReason}
                    >
                      <ActionIcon aria-hidden="true" />
                      {config.label}
                    </TooltipButton>
                  );
                }
                return (
                  <Button
                    key={action}
                    variant={variant}
                    size="sm"
                    onClick={() => requestAction(action)}
                  >
                    <ActionIcon aria-hidden="true" />
                    {config.label}
                  </Button>
                );
              })
            : null}
          {canWrite && canDelete(runner.admin_state) ? (
            <Button
              variant="destructive"
              size="sm"
              onClick={() => {
                setError(null);
                setConfirmDelete({ runner: runnerForDialog, ...DELETE_ACTION_CONFIG });
              }}
            >
              {DELETE_ACTION_CONFIG.label}
            </Button>
          ) : null}
          {grafanaHref ? (
            <Button asChild variant="outline" size="sm">
              <a href={grafanaHref} target="_blank" rel="noreferrer">
                {OPEN_GRAFANA_LABEL} <ExternalLinkIcon size={12} aria-hidden="true" />
              </a>
            </Button>
          ) : null}
          {/* Manual re-read, chosen over polling: the platform admin decides
              when the page is stale. Rides the same router refresh every
              action above already ends on. */}
          <IconAction label={REFRESH_RUNNER_LABEL} onClick={() => router.refresh()}>
            <RefreshCwIcon aria-hidden="true" />
          </IconAction>
        </div>
      </div>

      {/* The self-test refusal reads here, beside the control that asked for
          it — the other two actions carry their errors inside their confirm
          dialog, which a self-test never opens. */}
      {selftestError ? <Alert variant="destructive" className="mb-md">{selftestError}</Alert> : null}

      <div className="mb-2xl flex flex-col gap-md">
        <div className="flex flex-wrap items-center gap-2xl text-body-sm text-muted-foreground">
          <span className="inline-flex items-center gap-md">
            <RunnerStatus adminState={runner.admin_state} liveness={runner.liveness} />
            <a
              href={RUNNER_STATES_DOC_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-pulse underline-offset-2 hover:underline focus-visible:underline"
            >
              <CircleHelpIcon size={13} aria-hidden="true" />
              Learn more<span className="sr-only"> about runner states (opens in a new tab)</span>
            </a>
          </span>
          <span className="inline-flex flex-wrap gap-sm">
            <Badge>{SANDBOX_TIER_LABELS[runner.sandbox_tier]}</Badge>
            {runner.degraded ? <Badge variant="error">{DEGRADED_BADGE_LABEL}</Badge> : null}
            {runner.labels.map((label) => (
              <Badge key={label}>{label}</Badge>
            ))}
          </span>
        </div>
        {/* The mismatch line renders ONLY when a real verdict contradicts a real
            assignment: the reason names the specific missing mechanism, and the
            achievable line states what the host reported — assigned against
            achievable, side by side (Dimensions 4.1 / 4.2). */}
        {runner.degraded && runner.degraded_reason ? (
          <p className="font-mono text-body-sm text-destructive">
            {ASSIGNMENT_UNMET_PREFIX}
            {runner.degraded_reason}
            {runner.achievable ? ` · ${describeAchievable(runner.achievable)}` : ""}
          </p>
        ) : null}
      </div>

      <RunnerActionConfirm
        target={confirmAction}
        error={error}
        onOpenChange={() => setConfirmAction(null)}
        onConfirm={runAction}
      />
      <RunnerActionConfirm
        target={confirmDelete}
        error={error}
        onOpenChange={() => setConfirmDelete(null)}
        onConfirm={runDelete}
      />
    </div>
  );
}
