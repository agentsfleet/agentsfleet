"use client";

import { useOptimistic, useState, useTransition } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { ExternalLinkIcon, RefreshCwIcon } from "lucide-react";
import { Alert, Button, CopyButton, TooltipButton } from "@agentsfleet/design-system";
import {
  RUNNER_ADMIN_ACTION,
  RUNNER_ADMIN_STATE,
  type RunnerAdminState,
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
import { RunnerIdentityLine } from "./RunnerIdentityLine";
import {
  COPY_RUNNER_ID_LABEL,
  OPEN_GRAFANA_LABEL,
  REFRESH_RUNNER_LABEL,
  RUNNER_ACTIONS_LABEL,
  RUNNER_BREADCRUMB_LABEL,
  RUNNERS_CRUMB_LABEL,
} from "./runner-copy";

// The FleetHeader shape verbatim: breadcrumb left, actions right, one
// vertically-centred row, and NO second title — the breadcrumb already names
// the host and the page's <h1> is screen-reader-only. The runner id is the
// paste-into-a-ticket value, so it rides a CopyButton beside the breadcrumb
// rather than 26 characters of visible text. Identity — status, isolation
// tier, labels — is one line below; enrolment is not repeated here because
// Activity's registered record carries it with the real date.

const SelftestIcon = SELFTEST_ACTION_CONFIG.icon;

// The state each PATCH verb moves the runner into — what the badge paints the
// instant the operator confirms, before the daemon answers. The daemon remains
// the authority: its answer, or the refresh that follows, replaces the paint.
const OPTIMISTIC_ADMIN_STATE: Record<RunnerStateAction, RunnerAdminState> = {
  [RUNNER_ADMIN_ACTION.cordon]: RUNNER_ADMIN_STATE.cordoned,
  [RUNNER_ADMIN_ACTION.drain]: RUNNER_ADMIN_STATE.draining,
  [RUNNER_ADMIN_ACTION.revoke]: RUNNER_ADMIN_STATE.revoked,
};

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
  // Painted at confirm, reconciled inside the same transition: success refreshes
  // the server tree, and a 409 or any failure ends the transition so the badge
  // falls back to the server-rendered state on its own.
  const [adminState, paintAdminState] = useOptimistic(runner.admin_state);

  function requestAction(action: RunnerStateAction) {
    setError(null);
    setConfirmAction({ runner, action, ...ACTION_CONFIG[action] });
  }

  // Both confirm-backed actions resolve when their transition settles, so the
  // dialog (RunnerActionConfirm forwards the promise to ConfirmDialog) holds
  // its buttons disabled and reads "Working…" until the daemon has answered —
  // the kill switch's shape. A confirm that returned at once would leave the
  // dialog live beside a badge already painted, and a second click would send
  // a second PATCH against a state the page already claims.
  function settled(work: () => Promise<void>): Promise<void> {
    return new Promise<void>((resolve) => {
      startTransition(async () => {
        try {
          await work();
        } finally {
          resolve();
        }
      });
    });
  }

  function runAction(target: NonNullable<RunnerActionConfirmTarget>): Promise<void> {
    return settled(async () => {
      paintAdminState(OPTIMISTIC_ADMIN_STATE[target.action]);
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

  function runDelete(target: NonNullable<RunnerDeleteConfirmTarget>): Promise<void> {
    return settled(async () => {
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
          className="flex min-w-0 items-center text-sm text-muted-foreground"
        >
          <Link href={runnersIndexPath()} className="hover:text-foreground">
            {RUNNERS_CRUMB_LABEL}
          </Link>
          <span aria-hidden="true" className="mx-md">/</span>
          <span className="truncate font-mono text-foreground">{runner.host_id}</span>
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
          {canWrite && canSelftest(adminState) ? (
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
            ? actionsFor(adminState).map((action) => {
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
          {/* A destructive control renders from the state the server confirmed,
              never from the optimistic paint: Delete must not appear while the
              revoke that would allow it is still unanswered. */}
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
          <TooltipButton size="sm" variant="outline" className="aspect-square px-0" aria-label={REFRESH_RUNNER_LABEL} tooltip={REFRESH_RUNNER_LABEL} onClick={() => router.refresh()}>
            <RefreshCwIcon aria-hidden="true" />
          </TooltipButton>
        </div>
      </div>

      {/* The self-test refusal reads here, beside the control that asked for
          it — the other two actions carry their errors inside their confirm
          dialog, which a self-test never opens. */}
      {selftestError ? <Alert variant="destructive" className="mb-md">{selftestError}</Alert> : null}

      <RunnerIdentityLine runner={runner} adminState={adminState} />

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
