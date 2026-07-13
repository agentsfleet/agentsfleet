"use client";

import { type ReactNode, type Ref, useImperativeHandle, useRef, useState, useTransition } from "react";
import {
  Badge,
  type BadgeVariant,
  Button,
  CopyButton,
  DataTable,
  type DataTableColumn,
  EmptyState,
  IconAction,
  Time,
} from "@agentsfleet/design-system";
// Glyph vocabulary follows the settings surfaces: BanIcon means "revoke a
// credential", Trash2Icon means "delete". There is no runner delete, so no
// trash-can appears here — a trash-can that revokes would advertise an action
// the daemon does not serve.
import { ActivityIcon, BanIcon, ArrowDownToLineIcon, PauseIcon, ServerIcon } from "lucide-react";
import {
  RUNNER_ADMIN_ACTION,
  RUNNER_ADMIN_STATE,
  SANDBOX_TIER_LABELS,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type RunnerListResponse,
  type RunnerListItem,
  type RunnerAdminAction,
  type RunnerAdminState,
  type RunnerEventsResponse,
  type RunnerLiveness,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { listRunnersAction, listRunnerEventsAction, updateRunnerAdminStateAction } from "../actions";
import { RunnerActionConfirm, RunnerActivityDialog, type RunnerActionConfirmTarget } from "./RunnerDialogs";

// Derived liveness → badge colour. registered = not yet connected (amber);
// online = idle + reachable (green); busy = holding a live lease (cyan); offline
// = heartbeat lapsed (muted default).
const LIVENESS_VARIANT: Record<RunnerLiveness, BadgeVariant> = {
  registered: "amber",
  online: "green",
  busy: "cyan",
  offline: "default",
};

const ADMIN_STATE_VARIANT: Record<RunnerAdminState, BadgeVariant> = {
  active: "green",
  cordoned: "amber",
  draining: "cyan",
  drained: "default",
  revoked: "destructive",
};

// Each admin action carries its confirm-dialog copy plus the glyph its
// icon-only row trigger renders. `label` is the single source of the accessible
// name — IconAction feeds it to both the aria-label and the tooltip body — so
// the icon choice is presentation only; correctness rides on `label`.
const ACTION_CONFIG: Record<RunnerAdminAction, {
  label: string;
  title: string;
  description: string;
  intent: "default" | "destructive";
  errorAction: string;
  icon: ReactNode;
}> = {
  [RUNNER_ADMIN_ACTION.cordon]: {
    label: "Cordon",
    title: "Cordon this runner?",
    description: "Runner-plane calls stop immediately. Existing lease rows stay fenced until expiry or reassignment.",
    intent: "default",
    errorAction: "cordon this runner",
    icon: <PauseIcon />,
  },
  [RUNNER_ADMIN_ACTION.drain]: {
    label: "Drain",
    title: "Drain this runner?",
    description: "The runner stops taking new work and becomes drained automatically once active leases reach zero.",
    intent: "default",
    errorAction: "drain this runner",
    icon: <ArrowDownToLineIcon />,
  },
  [RUNNER_ADMIN_ACTION.revoke]: {
    label: "Revoke",
    title: "Revoke this runner?",
    description: "The runner token is blocked immediately. This is terminal for the enrolled host.",
    intent: "destructive",
    errorAction: "revoke this runner",
    icon: <BanIcon />,
  },
};

function actionsFor(state: RunnerAdminState): RunnerAdminAction[] {
  const out: RunnerAdminAction[] = [];
  if (state === RUNNER_ADMIN_STATE.active) out.push(RUNNER_ADMIN_ACTION.cordon);
  if (state === RUNNER_ADMIN_STATE.active || state === RUNNER_ADMIN_STATE.cordoned) out.push(RUNNER_ADMIN_ACTION.drain);
  if (state !== RUNNER_ADMIN_STATE.revoked) out.push(RUNNER_ADMIN_ACTION.revoke);
  return out;
}

export type RunnerListHandle = { refresh: () => void };

type ActivityDataState = {
  runnerId: string;
  data: RunnerEventsResponse;
};

function HostCell({ r }: { r: RunnerListItem }) {
  return (
    <div className="min-w-0">
      {/* The host id is truncated to fit the column, so the eye cannot recover it
          — the clipboard is the only way to get the whole value out of this cell. */}
      <div className="flex min-w-0 items-center gap-1">
        <div className="truncate font-mono text-sm">{r.host_id}</div>
        <CopyButton value={r.host_id} label={`Copy host id: ${r.host_id}`} />
      </div>
      <div className="font-mono text-xs tabular-nums text-muted-foreground">
        enrolled <Time value={new Date(r.created_at)} format="relative" /> ·{" "}
        {r.last_seen_at > 0 ? (
          <>
            last seen <Time value={new Date(r.last_seen_at)} format="relative" />
          </>
        ) : (
          "never connected"
        )}
      </div>
    </div>
  );
}

function StatusCell({ r }: { r: RunnerListItem }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      <Badge variant={LIVENESS_VARIANT[r.liveness]}>{r.liveness}</Badge>
      <Badge variant={ADMIN_STATE_VARIANT[r.admin_state]}>{r.admin_state}</Badge>
    </div>
  );
}

function LabelsCell({ r }: { r: RunnerListItem }) {
  if (r.labels.length === 0) return <span className="text-xs text-muted-foreground">—</span>;
  return (
    <div className="flex flex-wrap gap-1.5" aria-label={`${r.host_id} labels`}>
      {r.labels.map((label) => <Badge key={label} variant="default">{label}</Badge>)}
    </div>
  );
}

function ActionsCell({
  r,
  pending,
  onActivity,
  onAction,
}: {
  r: RunnerListItem;
  pending: boolean;
  onActivity: (runner: RunnerListItem) => void;
  onAction: (runner: RunnerListItem, action: RunnerAdminAction) => void;
}) {
  return (
    <div className="flex flex-wrap items-center justify-end gap-2">
      <IconAction label="Activity" onClick={() => onActivity(r)} disabled={pending}>
        <ActivityIcon />
      </IconAction>
      {actionsFor(r.admin_state).map((action) => (
        <IconAction
          key={action}
          label={ACTION_CONFIG[action].label}
          variant={action === RUNNER_ADMIN_ACTION.revoke ? "destructive" : "outline"}
          onClick={() => onAction(r, action)}
          disabled={pending}
        >
          {ACTION_CONFIG[action].icon}
        </IconAction>
      ))}
    </div>
  );
}

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
