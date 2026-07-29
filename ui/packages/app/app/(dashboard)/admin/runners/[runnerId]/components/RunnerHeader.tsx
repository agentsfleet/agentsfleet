"use client";

import { useState, useTransition } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import { ExternalLinkIcon } from "lucide-react";
import { Badge, Button, CopyButton } from "@agentsfleet/design-system";
import {
  SANDBOX_TIER_LABELS,
  type RunnerAdminAction,
  type RunnerDetail,
  type RunnerListItem,
} from "@/lib/api/runners";
import { runnersIndexPath } from "@/lib/runner-routes";
import { presentErrorString } from "@/lib/errors";
import {
  ACTION_CONFIG,
  DELETE_ACTION_CONFIG,
  actionsFor,
  canDelete,
} from "../../components/RunnerListCells";
import {
  RunnerActionConfirm,
  type RunnerActionConfirmTarget,
  type RunnerDeleteConfirmTarget,
} from "../../components/RunnerDialogs";
import { updateRunnerAdminStateAction, deleteRunnerAction } from "../../actions";
import { RunnerStatus } from "../../components/RunnerStatus";
import {
  COPY_RUNNER_ID_LABEL,
  OPEN_GRAFANA_LABEL,
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

export function RunnerHeader({
  runner,
  grafanaHref,
}: {
  runner: RunnerDetail;
  grafanaHref: string | null;
}) {
  const router = useRouter();
  const [confirmAction, setConfirmAction] = useState<RunnerActionConfirmTarget>(null);
  const [confirmDelete, setConfirmDelete] = useState<RunnerDeleteConfirmTarget>(null);
  const [error, setError] = useState<string | null>(null);
  const [, startTransition] = useTransition();

  function requestAction(action: RunnerAdminAction) {
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
          {actionsFor(runner.admin_state).map((action) => (
            <Button
              key={action}
              variant={ACTION_CONFIG[action].intent === "destructive" ? "destructive" : "outline"}
              size="sm"
              onClick={() => requestAction(action)}
            >
              {ACTION_CONFIG[action].label}
            </Button>
          ))}
          {canDelete(runner.admin_state) ? (
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
        </div>
      </div>

      <div className="mb-2xl flex flex-wrap items-center gap-2xl text-body-sm text-muted-foreground">
        <RunnerStatus adminState={runner.admin_state} liveness={runner.liveness} />
        <span className="inline-flex flex-wrap gap-sm">
          <Badge>{SANDBOX_TIER_LABELS[runner.sandbox_tier]}</Badge>
          {runner.labels.map((label) => (
            <Badge key={label}>{label}</Badge>
          ))}
        </span>
      </div>

      <RunnerActionConfirm
        target={confirmAction}
        error={error}
        onOpenChange={(open) => {
          if (!open) setConfirmAction(null);
        }}
        onConfirm={runAction}
      />
      <RunnerActionConfirm
        target={confirmDelete}
        error={error}
        onOpenChange={(open) => {
          if (!open) setConfirmDelete(null);
        }}
        onConfirm={runDelete}
      />
    </div>
  );
}
