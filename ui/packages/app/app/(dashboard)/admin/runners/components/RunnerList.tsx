"use client";

import { type Ref, useImperativeHandle, useRef, useState, useTransition } from "react";
import {
  Badge,
  DataTable,
  type DataTableColumn,
  EmptyState,
  PAGINATION_KIND,
} from "@agentsfleet/design-system";
import { ServerIcon } from "lucide-react";
import {
  SANDBOX_TIER_LABELS,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type RunnerListResponse,
  type RunnerListItem,
  type RunnerSort,
  type RunnerAdminAction,
  type RunnerEventsResponse,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { listRunnersAction, listRunnerEventsAction, updateRunnerAdminStateAction, deleteRunnerAction } from "../actions";
import {
  RunnerActionConfirm,
  RunnerActivityDialog,
  type RunnerActionConfirmTarget,
  type RunnerConfirmCopy,
  type RunnerDeleteConfirmTarget,
} from "./RunnerDialogs";
import { ACTION_CONFIG, DELETE_ACTION_CONFIG, ActionsCell, HostCell, LabelsCell, StatusCell } from "./RunnerListCells";

export type RunnerListHandle = { refresh: () => void };

const HOST_SORT_ASCENDING: RunnerSort = "host_id";
const HOST_SORT_DESCENDING: RunnerSort = "-host_id";

type ActivityDataState = {
  runnerId: string;
  data: RunnerEventsResponse;
};

function buildColumns({
  pending,
  onActivity,
  onAction,
  onDelete,
}: {
  pending: boolean;
  onActivity: (runner: RunnerListItem) => void;
  onAction: (runner: RunnerListItem, action: RunnerAdminAction) => void;
  onDelete: (runner: RunnerListItem) => void;
}): DataTableColumn<RunnerListItem>[] {
  return [
    { key: "host", header: "Host", sortable: true, cell: (r) => <HostCell r={r} /> },
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
      cell: (r) => (
        <ActionsCell r={r} pending={pending} onActivity={onActivity} onAction={onAction} onDelete={onDelete} />
      ),
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
  const [sort, setSort] = useState<RunnerSort>(DEFAULT_SORT);
  const [error, setError] = useState<string | null>(null);
  const [confirmTarget, setConfirmTarget] = useState<RunnerActionConfirmTarget>(null);
  const [deleteTarget, setDeleteTarget] = useState<RunnerDeleteConfirmTarget>(null);
  const [activityRunner, setActivityRunner] = useState<RunnerListItem | null>(null);
  const [activityData, setActivityData] = useState<ActivityDataState | null>(null);
  const [activityError, setActivityError] = useState<string | null>(null);
  const activityRunnerIdRef = useRef<string | null>(null);

  // The header "Create runner" dialog (rendered by the parent view) calls this
  // via ref on create — a targeted re-fetch of page 1 (newest-first default).
  useImperativeHandle(ref, () => ({ refresh: () => loadPage(1, false, DEFAULT_SORT) }));

  function loadPage(nextPage: number, retried = false, nextSort = sort) {
    startTransition(async () => {
      const r = await listRunnersAction({ page: nextPage, page_size: DEFAULT_PAGE_SIZE, sort: nextSort });
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load runners" }));
        if (r.errorCode === "UZ-REQ-001" && !retried) loadPage(1, true, DEFAULT_SORT);
        return;
      }
      setError(null);
      setItems(r.data.items);
      setTotal(r.data.total);
      setPage(r.data.page);
      setSort(nextSort);
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

  function confirmDelete(target: RunnerConfirmCopy) {
    startTransition(async () => {
      const r = await deleteRunnerAction(target.runner.id);
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: target.errorAction }));
        return;
      }
      setError(null);
      setDeleteTarget(null);
      // Refetch rather than splice the row out: the row is gone server-side, so
      // the current page is now short one entry and `total` has moved. Refetching
      // keeps pagination honest where a local filter would leave a gap.
      loadPage(page);
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
    onDelete: (runner) => {
      setError(null);
      setDeleteTarget({ runner, ...DELETE_ACTION_CONFIG });
    },
  });

  return (
    <div className="space-y-4">
      <DataTable
        columns={columns}
        rows={items}
        rowKey={(r) => r.id}
        caption="Runners"
        sortKey={sort === HOST_SORT_ASCENDING || sort === HOST_SORT_DESCENDING ? "host" : undefined}
        sortDirection={sort === HOST_SORT_DESCENDING ? "descending" : "ascending"}
        onSortChange={() => loadPage(
          1,
          false,
          sort === HOST_SORT_ASCENDING ? HOST_SORT_DESCENDING : HOST_SORT_ASCENDING,
        )}
        pagination={{
          kind: PAGINATION_KIND.page,
          page,
          pageSize: DEFAULT_PAGE_SIZE,
          total,
          totalLabel: "runners",
          onPageChange: loadPage,
          isLoading: pending,
        }}
        empty={
          <EmptyState
            icon={<ServerIcon size={28} />}
            title="No runners yet"
            description="Add a host to run fleets."
          />
        }
      />

      {error && !confirmTarget && !deleteTarget ? <p className="text-sm text-destructive">{error}</p> : null}

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
      <RunnerActionConfirm
        target={deleteTarget}
        error={error}
        onOpenChange={() => {
          setDeleteTarget(null);
          setError(null);
        }}
        onConfirm={confirmDelete}
      />
    </div>
  );
}
