"use client";

import { type Ref, useImperativeHandle, useRef, useState, useTransition } from "react";
import {
  Badge,
  type BadgeVariant,
  Button,
  EmptyState,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import { ActivityIcon, ServerIcon } from "lucide-react";
import {
  RUNNER_ADMIN_ACTION,
  RUNNER_ADMIN_STATE,
  RUNNER_SORTS,
  DEFAULT_PAGE_SIZE,
  DEFAULT_SORT,
  type RunnerListResponse,
  type RunnerListItem,
  type RunnerAdminAction,
  type RunnerAdminState,
  type RunnerEventsResponse,
  type RunnerLiveness,
  type RunnerSort,
} from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { listRunnersAction, listRunnerEventsAction, updateRunnerAdminStateAction } from "../actions";
import { RunnerActionConfirm, RunnerActivityDialog, type RunnerActionConfirmTarget } from "./RunnerDialogs";

const SORT_LABELS: Record<RunnerSort, string> = {
  "-created_at": "Newest first",
  created_at: "Oldest first",
  host_id: "Host A–Z",
  "-host_id": "Host Z–A",
};

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

const ACTION_CONFIG: Record<RunnerAdminAction, {
  label: string;
  title: string;
  description: string;
  intent: "default" | "destructive";
  errorAction: string;
}> = {
  [RUNNER_ADMIN_ACTION.cordon]: {
    label: "Cordon",
    title: "Cordon this runner?",
    description: "Runner-plane calls stop immediately. Existing lease rows stay fenced until expiry or reassignment.",
    intent: "default",
    errorAction: "cordon this runner",
  },
  [RUNNER_ADMIN_ACTION.drain]: {
    label: "Drain",
    title: "Drain this runner?",
    description: "The runner stops taking new work and becomes drained automatically once active leases reach zero.",
    intent: "default",
    errorAction: "drain this runner",
  },
  [RUNNER_ADMIN_ACTION.revoke]: {
    label: "Revoke",
    title: "Revoke this runner?",
    description: "The runner token is blocked immediately. This is terminal for the enrolled host.",
    intent: "destructive",
    errorAction: "revoke this runner",
  },
};

function fmt(ms: number): string {
  return new Date(ms).toLocaleString();
}

export type RunnerListHandle = { refresh: () => void };

type ActivityDataState = {
  runnerId: string;
  data: RunnerEventsResponse;
};

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
  const [activityRunner, setActivityRunner] = useState<RunnerListItem | null>(null);
  const [activityData, setActivityData] = useState<ActivityDataState | null>(null);
  const [activityError, setActivityError] = useState<string | null>(null);
  const activityRunnerIdRef = useRef<string | null>(null);

  // The header "Add runner" dialog (rendered by the parent view) calls this via
  // ref on create — a targeted re-fetch of page 1, not a full-route refresh.
  useImperativeHandle(ref, () => ({
    refresh: () => loadPage({ page: 1, sort: DEFAULT_SORT }),
  }));

  const lastPage = Math.max(1, Math.ceil(total / DEFAULT_PAGE_SIZE));

  function apply(data: RunnerListResponse, nextSort: RunnerSort) {
    setItems(data.items);
    setTotal(data.total);
    setPage(data.page);
    setSort(nextSort);
  }

  function updateLocalAdminState(runnerId: string, adminState: RunnerAdminState) {
    setItems((rows) => rows.map((row) => (row.id === runnerId ? { ...row, admin_state: adminState } : row)));
  }

  function loadPage(next: { page: number; sort?: RunnerSort }, retried = false) {
    const nextPage = next.page;
    const nextSort = next.sort ?? sort;
    startTransition(async () => {
      const r = await listRunnersAction({ page: nextPage, page_size: DEFAULT_PAGE_SIZE, sort: nextSort });
      if (!r.ok) {
        setError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "load runners" }));
        if (r.errorCode === "UZ-REQ-001" && !retried) loadPage({ page: 1, sort: DEFAULT_SORT }, true);
        return;
      }
      setError(null);
      apply(r.data, nextSort);
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
      updateLocalAdminState(target.runner.id, r.data.admin_state);
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

  return (
    <div className="space-y-4">
      {items.length > 0 ? (
        <div className="flex flex-wrap items-center justify-between gap-3">
          <Select value={sort} onValueChange={(v) => loadPage({ sort: v as RunnerSort, page: 1 })}>
            <SelectTrigger className="w-44" aria-label="Sort runners">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {RUNNER_SORTS.map((s) => (
                <SelectItem key={s} value={s}>
                  {SORT_LABELS[s]}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      ) : null}

      {items.length === 0 ? (
        <EmptyState
          icon={<ServerIcon size={28} />}
          title="No runners yet"
          description="Add a host to run agent work."
        />
      ) : (
        <div className="divide-y rounded-md border">
          {items.map((r) => (
            <Row
              key={r.id}
              r={r}
              pending={pending}
              onActivity={openActivity}
              onAction={(action) => {
                setError(null);
                setConfirmTarget({ runner: r, action, ...ACTION_CONFIG[action] });
              }}
            />
          ))}
        </div>
      )}

      {error && !confirmTarget ? <p className="text-sm text-destructive">{error}</p> : null}

      {lastPage > 1 ? (
        <div className="flex items-center justify-between text-sm text-muted-foreground">
          <span>
            Page {page} of {lastPage} · {total} runners
          </span>
          <div className="flex gap-2">
            <Button type="button" variant="ghost" size="sm" disabled={pending || page <= 1} onClick={() => loadPage({ page: page - 1 })}>
              Previous
            </Button>
            <Button type="button" variant="ghost" size="sm" disabled={pending || page >= lastPage} onClick={() => loadPage({ page: page + 1 })}>
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

function Row({
  r,
  pending,
  onActivity,
  onAction,
}: {
  r: RunnerListItem;
  pending: boolean;
  onActivity: (runner: RunnerListItem) => void;
  onAction: (action: RunnerAdminAction) => void;
}) {
  const actions = actionsFor(r.admin_state);
  return (
    <div className="flex flex-col gap-3 p-3 sm:flex-row sm:items-center sm:justify-between" aria-label={`${r.host_id} runner row`}>
      <div className="min-w-0 space-y-2">
        <div className="flex flex-wrap items-center gap-2">
          <span className="truncate font-mono text-sm">{r.host_id}</span>
          <Badge variant={LIVENESS_VARIANT[r.liveness]}>{r.liveness}</Badge>
          <Badge variant={ADMIN_STATE_VARIANT[r.admin_state]}>{r.admin_state}</Badge>
          <Badge variant="default">{r.sandbox_tier}</Badge>
        </div>
        <div className="font-mono text-xs tabular-nums text-muted-foreground">
          enrolled {fmt(r.created_at)} ·{" "}
          {r.last_seen_at > 0 ? `last seen ${fmt(r.last_seen_at)}` : "never connected"}
        </div>
        {r.labels.length > 0 ? (
          <div className="flex flex-wrap gap-1.5" aria-label={`${r.host_id} labels`}>
            {r.labels.map((label) => <Badge key={label} variant="default">{label}</Badge>)}
          </div>
        ) : null}
      </div>
      <div className="flex flex-wrap items-center gap-2">
        <Button type="button" variant="outline" size="sm" onClick={() => onActivity(r)} disabled={pending}>
          <ActivityIcon />
          Activity
        </Button>
        <div className="flex flex-wrap gap-2">
          {actions.map((action) => (
            <Button
              key={action}
              type="button"
              variant={action === RUNNER_ADMIN_ACTION.revoke ? "destructive" : "ghost"}
              size="sm"
              onClick={() => onAction(action)}
              disabled={pending}
            >
              {ACTION_CONFIG[action].label}
            </Button>
          ))}
        </div>
      </div>
    </div>
  );
}

function actionsFor(state: RunnerAdminState): RunnerAdminAction[] {
  const out: RunnerAdminAction[] = [];
  if (state === RUNNER_ADMIN_STATE.active) out.push(RUNNER_ADMIN_ACTION.cordon);
  if (state === RUNNER_ADMIN_STATE.active || state === RUNNER_ADMIN_STATE.cordoned) out.push(RUNNER_ADMIN_ACTION.drain);
  if (state !== RUNNER_ADMIN_STATE.revoked) out.push(RUNNER_ADMIN_ACTION.revoke);
  return out;
}
