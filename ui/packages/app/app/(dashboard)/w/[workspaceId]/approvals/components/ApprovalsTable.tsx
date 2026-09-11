"use client";

import type * as React from "react";
import Link from "next/link";
import {
  Badge,
  DataTable,
  IconAction,
  Time,
  type DataTableColumn,
} from "@agentsfleet/design-system";
import { CheckIcon, XIcon } from "lucide-react";

import type { ApprovalGate } from "@/lib/api/approvals";
import { APPROVAL_STATUS, type ApprovalStatusTag } from "@/lib/api/approvals-types";
import { AgentLabel } from "@/components/domain/AgentLabel";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { PersonLabel } from "@/components/domain/PersonLabel";
import { workspacePath } from "@/lib/workspace-routes";
import {
  ACTIONS_COLUMN_HEADER,
  APPROVALS_TABLE_CAPTION,
  APPROVE_LABEL,
  AWAITING_DECISION,
  DECIDED_COLUMN_HEADER,
  DENY_LABEL,
  FLEET_COLUMN_HEADER,
  REQUESTED_COLUMN_HEADER,
  REQUEST_COLUMN_HEADER,
  STATUS_COLUMN_HEADER,
  STATUS_LABEL,
  STATUS_VARIANT,
} from "../copy";

const TIME_CELL_CLASS = "font-mono text-xs tabular-nums text-muted-foreground";
const ICON_SIZE = 14;
const PAGE_SIZE = 25;

/**
 * The row's own status, as one of the five the API can return.
 *
 * A spelling this build has no arm for is shown verbatim rather than guessed
 * at or hidden: an unknown status is a row somebody still has to understand,
 * and silently calling it pending would be the worse of the two mistakes.
 */
function statusOf(gate: ApprovalGate): ApprovalStatusTag | null {
  return gate.status in STATUS_LABEL ? (gate.status as ApprovalStatusTag) : null;
}

function statusLabel(gate: ApprovalGate): string {
  const tag = statusOf(gate);
  return tag === null ? gate.status : STATUS_LABEL[tag];
}

/** Only a row still waiting on a person can be approved or denied. */
function isPending(gate: ApprovalGate): boolean {
  return gate.status === APPROVAL_STATUS.PENDING;
}

/** What the row calls the request, and what the action labels name. */
function requestName(gate: ApprovalGate): string {
  return gate.proposed_action || `${gate.tool_name}:${gate.action_name}`;
}

type RowActions = {
  onApprove: (gateId: string) => void;
  /** Takes the whole gate: the confirm names the request it is about to end. */
  onDeny: (gate: ApprovalGate) => void;
};

function RequestCell({ gate, workspaceId }: { gate: ApprovalGate; workspaceId: string }) {
  return (
    <div className="min-w-0">
      <Link
        href={workspacePath(workspaceId, `approvals/${gate.gate_id}`)}
        className="truncate font-medium hover:underline"
      >
        {requestName(gate)}
      </Link>
      {gate.blast_radius ? (
        <div className="text-xs text-muted-foreground">{gate.blast_radius}</div>
      ) : null}
    </div>
  );
}

function FleetCell({ gate, workspaceId }: { gate: ApprovalGate; workspaceId: string }) {
  return (
    <div className="flex min-w-0 flex-col items-start gap-1">
      <Link href={workspacePath(workspaceId, `fleets/${gate.fleet_id}`)} className="hover:underline">
        <AgentLabel fleetId={gate.fleet_id} className="text-muted-foreground" />
      </Link>
      {gate.gate_kind ? <Badge variant="default">{gate.gate_kind}</Badge> : null}
    </div>
  );
}

function StatusCell({ gate }: { gate: ApprovalGate }) {
  const tag = statusOf(gate);
  return (
    <Badge variant={tag === null ? "default" : STATUS_VARIANT[tag]} className="normal-case">
      {statusLabel(gate)}
    </Badge>
  );
}

/**
 * The cell that swaps with the row's own state, which is the point of showing
 * every status in one table: a pending row's future (when it auto-denies) and a
 * settled row's past (when, and by whom) belong in the same place, because an
 * operator reading down the column is asking one question — what happened to
 * this, or what is about to.
 */
function DecidedCell({ gate }: { gate: ApprovalGate }) {
  if (isPending(gate)) {
    return (
      <div className="flex flex-col">
        <span className="text-xs text-muted-foreground">{AWAITING_DECISION}</span>
        <Time value={new Date(gate.timeout_at)} format="relative" className={TIME_CELL_CLASS} />
      </div>
    );
  }
  return (
    <div className="flex flex-col">
      {gate.updated_at === null ? null : (
        <Time value={new Date(gate.updated_at)} format="relative" className={TIME_CELL_CLASS} />
      )}
      {gate.resolved_by ? (
        <PersonLabel
          actor={gate.resolved_by}
          name={gate.resolved_by_name}
          className="text-xs text-muted-foreground"
        />
      ) : null}
    </div>
  );
}

/**
 * Approve resolves at the click; deny asks first. Denying is irreversible — the
 * grant is revoked and no later card can be raised for it — so it carries the
 * confirm step Secrets puts on delete. A settled row offers neither: there is
 * nothing left to decide.
 */
function ApprovalActions({ gate, actions }: { gate: ApprovalGate; actions: RowActions }) {
  if (!isPending(gate)) return null;
  const name = requestName(gate);
  return (
    <div className="flex justify-end gap-1">
      <IconAction
        type="button"
        onClick={() => actions.onApprove(gate.gate_id)}
        label={`${APPROVE_LABEL}: ${name}`}
        title={APPROVE_LABEL}
      >
        <CheckIcon size={ICON_SIZE} />
      </IconAction>
      <IconAction
        type="button"
        variant="destructive"
        onClick={() => actions.onDeny(gate)}
        label={`${DENY_LABEL}: ${name}`}
        title={DENY_LABEL}
      >
        <XIcon size={ICON_SIZE} />
      </IconAction>
    </div>
  );
}

function buildColumns({
  workspaceId,
  actions,
}: {
  workspaceId: string;
  actions: RowActions;
}): DataTableColumn<ApprovalGate>[] {
  return [
    {
      key: "fleet",
      header: FLEET_COLUMN_HEADER,
      sortValue: (g) => agentDisplayName(g.fleet_id),
      cell: (g) => <FleetCell gate={g} workspaceId={workspaceId} />,
    },
    {
      key: "request",
      header: REQUEST_COLUMN_HEADER,
      sortValue: requestName,
      cell: (g) => <RequestCell gate={g} workspaceId={workspaceId} />,
    },
    {
      key: "status",
      header: STATUS_COLUMN_HEADER,
      sortValue: statusLabel,
      cell: (g) => <StatusCell gate={g} />,
    },
    {
      key: "created_at",
      header: REQUESTED_COLUMN_HEADER,
      sortValue: (g) => g.created_at,
      cell: (g) => <Time value={new Date(g.created_at)} format="relative" className={TIME_CELL_CLASS} />,
    },
    {
      key: "decided",
      header: DECIDED_COLUMN_HEADER,
      hideOnMobile: true,
      sortValue: (g) => g.updated_at ?? g.timeout_at,
      cell: (g) => <DecidedCell gate={g} />,
    },
    {
      key: "actions",
      header: ACTIONS_COLUMN_HEADER,
      numeric: true,
      cell: (g) => <ApprovalActions gate={g} actions={actions} />,
    },
  ];
}

export function ApprovalsTable({
  workspaceId,
  gates,
  actions,
  empty,
}: {
  workspaceId: string;
  gates: ApprovalGate[];
  actions: RowActions;
  empty?: React.ReactNode;
}) {
  return (
    <DataTable
      columns={buildColumns({ workspaceId, actions })}
      rows={gates}
      rowKey={(g) => g.gate_id}
      caption={APPROVALS_TABLE_CAPTION}
      empty={empty}
      // The table's own pager, as every other table on the dashboard uses it.
      // It walks the rows already read — one page per status — which is why the
      // read is bounded rather than cursored: a workspace deep enough to exceed
      // it wants a filtered view, not a longer scroll.
      pagination={{ pageSize: PAGE_SIZE }}
    />
  );
}
