"use client";

import { type Ref, useImperativeHandle, useRef, useState, useTransition } from "react";
import {
  Badge,
  Button,
  DataTable,
  type DataTableColumn,
  EmptyState,
} from "@agentsfleet/design-system";
import { ServerIcon } from "lucide-react";
import {
  SANDBOX_TIER_LABELS,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type RunnerListResponse,
  type RunnerListItem,
  type RunnerAdminAction,
  type RunnerEventsResponse,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { listRunnersAction, listRunnerEventsAction, updateRunnerAdminStateAction } from "../actions";
import { RunnerActionConfirm, RunnerActivityDialog, type RunnerActionConfirmTarget } from "./RunnerDialogs";
import { ACTION_CONFIG, ActionsCell, HostCell, LabelsCell, StatusCell } from "./RunnerListCells";

export type RunnerListHandle = { refresh: () => void };

type ActivityDataState = {
  runnerId: string;
  data: RunnerEventsResponse;
};

function buildColumns({
  pending,
  onActivity,
  onAction,
}: {
  pending: boolean;
  onActivity: (runner: RunnerListItem) => void;
  onAction: (runner: RunnerListItem, action: RunnerAdminAction) => void;
}): DataTableColumn<RunnerListItem>[] {
  return [
    { key: "host", header: "Host", cell: (r) => <HostCell r={r} /> },
    { key: "status", header: "Status", cell: (r) => <StatusCell r={r} /> },
    {
      key: "isolation",
      header: "Isolation",
      hideOnMobile: true,
      cell: (r) => <Badge variant="default">{SANDBOX_TIER_LABELS[r.sandbox_tier]}</Badge>,
    },
    { key: "labels", header: "Labels", hideOnMobile: true, cell: (r) => <LabelsCell r={r} /> },
    {
      key: "actions",
      header: "Actions",
      numeric: true,
      cell: (r) => <ActionsCell r={r} pending={pending} onActivity={onActivity} onAction={onAction} />,
    },
  ];
}

export default function RunnerList({
  initial,
  ref,
}: {
  initial: RunnerListResponse;
  ref?: Ref<RunnerListHandle>;
}) {
  const [pending, startTransition] = useTransition();
  const [activityPending, startActivityTransition] = useTransition();
  const [items, setItems] = useState<RunnerListItem[]>(initial.items);
  const [total, setTotal] = useState(initial.total);
  const [page, setPage] = useState(initial.page);
  const [error, setError] = useState<string | null>(null);
  const [confirmTarget, setConfirmTarget] = useState<RunnerActionConfirmTarget>(null);
  const [activityRunner, setActivityRunner] = useState<RunnerListItem | null>(null);
  const [activityData, setActivityData] = useState<ActivityDataState | null>(null);
  const [activityError, setActivityError] = useState<string | null>(null);
  const activityRunnerIdRef = useRef<string | null>(null);

  // The header "Create runner" dialog (rendered by the parent view) calls this
  // via ref on create — a targeted re-fetch of page 1 (newest-first default).
  useImperativeHandle(ref, () => ({ refresh: () => loadPage(1) }));

  const lastPage = Math.max(1, Math.ceil(total / DEFAULT_PAGE_SIZE));

  function loadPage(nextPage: number, retried = false) {
    startTransition(async () => {
      const r = await listRunnersAction({ page: nextPage, page_size: DEFAULT_PAGE_SIZE, sort: DEFAULT_SORT });
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load runners" }));
        if (r.errorCode === "UZ-REQ-001" && !retried) loadPage(1, true);
        return;
      }
      setError(null);
      setItems(r.data.items);
      setTotal(r.data.total);
      setPage(r.data.page);
    });
  }

  function confirmAction(target: NonNullable<RunnerActionConfirmTarget>) {
    startTransition(async () => {
      const r = await updateRunnerAdminStateAction(target.runner.id, target.action);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: target.errorAction }));
        return;
      }
      setError(null);
      setItems((rows) => rows.map((row) => (row.id === target.runner.id ? { ...row, admin_state: r.data.admin_state } : row)));
      setConfirmTarget(null);
    });
  }

  function loadEvents(runnerId: string, nextPage = 1) {
    startActivityTransition(async () => {
      const r = await listRunnerEventsAction(runnerId, { page: nextPage, page_size: DEFAULT_PAGE_SIZE });
      if (activityRunnerIdRef.current !== runnerId) return;
      if (!r.ok) {
        setActivityError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load runner activity" }));
        return;
      }
      setActivityError(null);
      setActivityData({ runnerId, data: r.data });
    });
  }

  function openActivity(runner: RunnerListItem) {
    activityRunnerIdRef.current = runner.id;
    setActivityRunner(runner);
    setActivityData(null);
    setActivityError(null);
    loadEvents(runner.id);
  }

  function closeActivity() {
    activityRunnerIdRef.current = null;
    setActivityRunner(null);
    setActivityData(null);
    setActivityError(null);
  }

  const columns = buildColumns({
    pending,
    onActivity: openActivity,
    onAction: (runner, action) => {
      setError(null);
      setConfirmTarget({ runner, action, ...ACTION_CONFIG[action] });
    },
  });

  return (
    <div className="space-y-4">
      <DataTable
        columns={columns}
        rows={items}
        rowKey={(r) => r.id}
        caption="Runners"
        empty={
          <EmptyState
            icon={<ServerIcon size={28} />}
            title="No runners yet"
            description="Add a host to run fleets."
          />
        }
      />

      {error && !confirmTarget ? <p className="text-sm text-destructive">{error}</p> : null}

      {lastPage > 1 ? (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span>
            Page {page} of {lastPage} · {total} runners
          </span>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={pending || page <= 1} onClick={() => loadPage(page - 1)}>
              Previous
            </Button>
            <Button type="button" variant="ghost" size="sm" disabled={pending || page >= lastPage} onClick={() => loadPage(page + 1)}>
              Next
            </Button>
          </div>
        </div>
      ) : null}

      {activityRunner ? (
        <RunnerActivityDialog
          runner={activityRunner}
          data={activityData?.runnerId === activityRunner.id ? activityData.data : null}
          error={activityError}
          pending={activityPending}
          onOpenChange={(open) => {
            if (!open) closeActivity();
          }}
          onPage={(nextPage) => loadEvents(activityRunner.id, nextPage)}
        />
      ) : null}
      <RunnerActionConfirm
        target={confirmTarget}
        error={error}
        onOpenChange={() => {
          setConfirmTarget(null);
          setError(null);
        }}
        onConfirm={confirmAction}
      />
    </div>
  );
}
