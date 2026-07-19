// The row cells for the runner table, split from RunnerList.tsx when the
// host-id copy affordance pushed it past the 350-line cap — same shape as
// ModelsRegistryCells.tsx. RunnerList owns state and data flow; this module
// owns presentation.

import { type ReactNode } from "react";
import {
  Badge,
  type BadgeVariant,
  CopyButton,
  IconAction,
  Time,
} from "@agentsfleet/design-system";
// Glyph vocabulary follows the settings surfaces: BanIcon means "revoke a
// credential", Trash2Icon means "delete". Both appear here, and never together
// on one row — revoke is offered until the runner is revoked, delete only after
// (DELETE /v1/fleets/runners/{id} 409s on a live runner), exactly as ApiKeyList
// alternates the two.
import { ActivityIcon, BanIcon, ArrowDownToLineIcon, PauseIcon, Trash2Icon } from "lucide-react";
import {
  RUNNER_ADMIN_ACTION,
  RUNNER_ADMIN_STATE,
  type RunnerAdminAction,
  type RunnerAdminState,
  type RunnerListItem,
  type RunnerLiveness,
} from "@/lib/api/runners";

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
export const ACTION_CONFIG: Record<RunnerAdminAction, {
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

// Delete is deliberately NOT a member of ACTION_CONFIG: that map is keyed on
// RunnerAdminAction, the three PATCH verbs the daemon serves, and widening it
// would loosen an exhaustive type that actionsFor and RunnerList both lean on.
// Delete is a different HTTP verb with a different lifecycle, so it gets its own
// config and its own trigger.
export const DELETE_ACTION_CONFIG = {
  label: "Delete",
  title: "Delete this runner?",
  description:
    "Removes the revoked runner's record, along with its lease and event history. The enrolled host is unaffected — it was already blocked at revoke. This cannot be undone.",
  intent: "destructive" as const,
  errorAction: "delete this runner",
};

/** Only a revoked runner is deletable — the daemon 409s (UZ-RUN-016) otherwise. */
export function canDelete(state: RunnerAdminState): boolean {
  return state === RUNNER_ADMIN_STATE.revoked;
}

export function actionsFor(state: RunnerAdminState): RunnerAdminAction[] {
  const out: RunnerAdminAction[] = [];
  if (state === RUNNER_ADMIN_STATE.active) out.push(RUNNER_ADMIN_ACTION.cordon);
  if (state === RUNNER_ADMIN_STATE.active || state === RUNNER_ADMIN_STATE.cordoned) out.push(RUNNER_ADMIN_ACTION.drain);
  if (state !== RUNNER_ADMIN_STATE.revoked) out.push(RUNNER_ADMIN_ACTION.revoke);
  return out;
}


export function HostCell({ r }: { r: RunnerListItem }) {
  return (
    <div className="min-w-0">
      {/* Truncated in the cell; whole on the clipboard. */}
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

export function StatusCell({ r }: { r: RunnerListItem }) {
  return (
    <div className="flex flex-wrap gap-1.5">
      <Badge variant={LIVENESS_VARIANT[r.liveness]}>{r.liveness}</Badge>
      <Badge variant={ADMIN_STATE_VARIANT[r.admin_state]}>{r.admin_state}</Badge>
    </div>
  );
}

export function LabelsCell({ r }: { r: RunnerListItem }) {
  if (r.labels.length === 0) return <span className="text-xs text-muted-foreground">—</span>;
  return (
    <div className="flex flex-wrap gap-1.5" aria-label={`${r.host_id} labels`}>
      {r.labels.map((label) => <Badge key={label} variant="default">{label}</Badge>)}
    </div>
  );
}

export function ActionsCell({
  r,
  pending,
  onActivity,
  onAction,
  onDelete,
}: {
  r: RunnerListItem;
  pending: boolean;
  onActivity: (runner: RunnerListItem) => void;
  onAction: (runner: RunnerListItem, action: RunnerAdminAction) => void;
  onDelete: (runner: RunnerListItem) => void;
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
      {canDelete(r.admin_state) ? (
        <IconAction
          label={DELETE_ACTION_CONFIG.label}
          variant="destructive"
          onClick={() => onDelete(r)}
          disabled={pending}
        >
          <Trash2Icon />
        </IconAction>
      ) : null}
    </div>
  );
}

