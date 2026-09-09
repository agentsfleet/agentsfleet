"use client";

import {
  useCallback,
  useOptimistic,
  useState,
  useTransition,
  type ReactNode,
} from "react";
import {
  Alert,
  ConfirmDialog,
  EmptyState,
  SectionHeader,
} from "@agentsfleet/design-system";
import { CheckCircle2Icon } from "lucide-react";

import { RefreshButton } from "@/components/domain/RefreshButton";

import {
  approveApprovalAction,
  denyApprovalAction,
  listApprovalsAction,
} from "../actions";
import { type ApprovalGate, type ResolveOutcome } from "@/lib/api/approvals";
import {
  APPROVAL_DECISION,
  APPROVALS_PAGE_LIMIT,
  type ApprovalDecision,
} from "@/lib/api/approvals-types";
import { fallbackPersonLabel } from "@/lib/identity/person";
import { presentErrorString } from "@/lib/errors";
import type { ActionResult } from "@/lib/actions/with-token";
import {
  APPROVALS_SECTION_LABEL,
  DENY_CONFIRM_BODY,
  DENY_CONFIRM_TITLE,
  DENY_LABEL,
  NO_APPROVALS_DESCRIPTION,
  NO_APPROVALS_TITLE,
} from "../copy";
import { ApprovalsTable } from "./ApprovalsTable";

const EMPTY_ICON_SIZE = 28;
const SESSION_EXPIRED = "Session expired — refresh the page to sign back in.";

// The Server Action call itself rejecting (RSC transport down, the viewer
// offline) is a failure like any other to the row: the message comes back with
// `ok: false`. Left uncaught, a rejection inside an async transition reaches
// the error boundary and the whole inbox becomes an error page.
function rejectedCall(cause: unknown): ActionResult<ResolveOutcome> {
  return { ok: false, error: cause instanceof Error ? cause.message : String(cause) };
}

type Props = {
  workspaceId: string;
  initialItems: ApprovalGate[];
  initialCursor: string | null;
  /** When set, the list is filtered server-side by this fleet. */
  fleetId?: string;
};

export default function ApprovalsList({ workspaceId, initialItems, fleetId }: Props) {
  // The server rendered every row, in the order the table wants them. There is
  // no mount read to merge in and nothing to sort here: `SELECT_GATE_PAGE`
  // orders newest-first, which is what this table shows top-down.
  const [items, setItems] = useState<ApprovalGate[]>(initialItems);
  const [error, setError] = useState<string | null>(null);
  const [, startResolve] = useTransition();
  // The gate a denial is being confirmed for. Held here rather than in the row
  // so the dialog survives the row leaving the table optimistically.
  const [denyTarget, setDenyTarget] = useState<ApprovalGate | null>(null);
  // A resolved row leaves the inbox at the click, not at the answer. The base
  // list is updated on success; a failed resolve ends the transition and the
  // row comes back from that base on its own.
  const [visibleItems, hideGate] = useOptimistic(
    items,
    (current: ApprovalGate[], gateId: string) => current.filter((g) => g.gate_id !== gateId),
  );

  // Read when a person asks, and after a resolve. Never on a timer: a settled
  // row cannot change again, so a poll would spend a request every few seconds
  // to learn nothing — and the operator, not the page, decides when it is stale.
  //
  // One call, because the API answers every state from one query now. It was
  // five reads merged here, and before that five Server Actions from this
  // component, which Next ran one at a time.
  const read = useCallback(async (): Promise<ApprovalGate[] | null> => {
    const page = await listApprovalsAction(workspaceId, {
      limit: APPROVALS_PAGE_LIMIT,
      fleetId,
    });
    if (!page.ok) {
      setError(
        page.status === 401
          ? SESSION_EXPIRED
          : presentErrorString({
              errorCode: page.errorCode,
              message: page.error,
              action: "read the approvals",
            }),
      );
      return null;
    }
    return page.data.items;
  }, [workspaceId, fleetId]);

  const refresh = useCallback(async () => {
    setError(null);
    const fresh = await read();
    if (fresh !== null) setItems(fresh);
  }, [read]);

  function resolve(gateId: string, decision: ApprovalDecision) {
    setError(null);
    const isApprove = decision === APPROVAL_DECISION.APPROVE;
    const action = isApprove ? approveApprovalAction : denyApprovalAction;
    startResolve(async () => {
      hideGate(gateId);
      const result = await action(workspaceId, gateId).catch(rejectedCall);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: isApprove ? "approve this request" : "deny this request",
          }),
        );
        return;
      }
      const outcome: ResolveOutcome = result.data;
      if (outcome.kind === "already_resolved") {
        // A message, not a cell, so it carries the shortened subject rather
        // than waiting on a directory lookup nobody can hover anyway.
        setError(
          `Already ${outcome.data.outcome} by ${fallbackPersonLabel(outcome.data.resolved_by)}`,
        );
      }
      // The row does not leave the table, it changes state — so the answer is
      // read back and the row reappears under its new status, carrying who
      // decided it and when.
      const fresh = await read();
      if (fresh !== null) setItems(fresh);
    });
  }

  // What stands in for the table when it has no rows.
  //
  // No skeleton arm any more. The server rendered every state before this
  // component mounted, so an empty table IS an empty inbox — the claim is the
  // server's from the first paint, and there is no window in which "No
  // approvals yet" is a statement nobody has checked.
  //
  // It reads `items` rather than the optimistic list so a row that has only
  // just left keeps the table silent rather than announcing a state the server
  // has not confirmed.
  function emptyRegion(): ReactNode {
    if (items.length > 0 || error !== null) return <></>;
    return (
      <EmptyState
        icon={<CheckCircle2Icon size={EMPTY_ICON_SIZE} />}
        title={NO_APPROVALS_TITLE}
        description={NO_APPROVALS_DESCRIPTION}
      />
    );
  }

  function approve(gateId: string) {
    resolve(gateId, APPROVAL_DECISION.APPROVE);
  }

  function confirmDeny(gate: ApprovalGate): Promise<void> {
    setDenyTarget(null);
    resolve(gate.gate_id, APPROVAL_DECISION.DENY);
    return Promise.resolve();
  }

  return (
    <>
      {/* The re-read sits on the section's own line, the way "Install fleet"
          sits on Manage fleets — a control for the whole section belongs beside
          its name, not floating in the gap above the table. Rendered here
          rather than on the page because `refresh` is this component's state,
          and the page is a Server Component that cannot hold it. */}
      <SectionHeader className="mb-md" actions={<RefreshButton onRefresh={refresh} />}>
        {APPROVALS_SECTION_LABEL}
      </SectionHeader>

      <ApprovalsTable
        workspaceId={workspaceId}
        gates={visibleItems}
        actions={{ onApprove: approve, onDeny: setDenyTarget }}
        empty={emptyRegion()}
      />

      <ConfirmDialog
        open={denyTarget !== null}
        onOpenChange={() => setDenyTarget(null)}
        title={DENY_CONFIRM_TITLE}
        description={DENY_CONFIRM_BODY}
        confirmLabel={DENY_LABEL}
        intent="destructive"
        onConfirm={denyTarget ? () => confirmDeny(denyTarget) : undefined}
      />

      {error ? (
        <Alert variant="destructive" className="mt-3">{error}</Alert>
      ) : null}
    </>
  );
}
