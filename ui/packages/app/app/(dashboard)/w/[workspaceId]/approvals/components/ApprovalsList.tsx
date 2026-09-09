"use client";

import {
  useCallback,
  useEffect,
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
  Skeleton,
} from "@agentsfleet/design-system";
import { CheckCircle2Icon } from "lucide-react";

import { RefreshButton } from "@/components/domain/RefreshButton";

import {
  approveApprovalAction,
  denyApprovalAction,
  listAllApprovalsAction,
} from "../actions";
import { type ApprovalGate, type ResolveOutcome } from "@/lib/api/approvals";
import {
  APPROVAL_DECISION,
  APPROVAL_STATUS,
  APPROVAL_STATUS_ORDER,
  APPROVALS_PAGE_LIMIT,
  type ApprovalDecision,
  type ApprovalStatusTag,
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

/** The four states a row can no longer leave; the server renders the fifth. */
const SETTLED_STATUSES = APPROVAL_STATUS_ORDER.filter((s) => s !== APPROVAL_STATUS.PENDING);

function isPending(gate: ApprovalGate): boolean {
  return gate.status === APPROVAL_STATUS.PENDING;
}

/** Newest first, so the row a person just acted on is where they are looking. */
function byNewest(a: ApprovalGate, b: ApprovalGate): number {
  return b.created_at - a.created_at;
}

type Props = {
  workspaceId: string;
  initialItems: ApprovalGate[];
  initialCursor: string | null;
  /** When set, the list is filtered server-side by this fleet. */
  fleetId?: string;
};

export default function ApprovalsList({ workspaceId, initialItems, fleetId }: Props) {
  const [items, setItems] = useState<ApprovalGate[]>(initialItems);
  const [error, setError] = useState<string | null>(null);
  // Whether the load's settled read has come back. Until it has, the table is
  // showing the server's pending page and nothing else.
  const [settledRead, setSettledRead] = useState(false);
  const [, startRead] = useTransition();
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

  // The API narrows to ONE status per read and defaults to pending, so a table
  // holding every state means one read per state, merged here. They are asked
  // for together rather than behind tabs: what a fleet was allowed and what it
  // was refused are the same question as what it is asking now.
  //
  // Read on load and when a person asks, never on a timer. A settled row cannot
  // change again, so a poll would spend four requests every few seconds to
  // learn nothing — and the operator, not the page, decides when it is stale.
  const readStatuses = useCallback(async (
    statuses: readonly ApprovalStatusTag[],
  ): Promise<ApprovalGate[] | null> => {
    // ONE server action, which fans the statuses out on the server. Asking for
    // them from here looked parallel — `Promise.all` over five calls — and was
    // not: Next runs Server Actions one at a time per client, so five became
    // five sequential round trips, measured at 4.6s from load to full table.
    //
    // A single failed page would silently shorten the table, so the read stays
    // all or nothing: the rows already shown stand until a whole one succeeds.
    const page = await listAllApprovalsAction(
      workspaceId,
      { limit: APPROVALS_PAGE_LIMIT, fleetId },
      statuses,
    );
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

  const read = useCallback(
    async () => {
      const all = await readStatuses(APPROVAL_STATUS_ORDER);
      return all === null ? null : [...all].sort(byNewest);
    },
    [readStatuses],
  );

  const refresh = useCallback(async () => {
    setError(null);
    const fresh = await read();
    if (fresh !== null) setItems(fresh);
  }, [read]);

  // The server already rendered the pending page, so the load only fetches the
  // four states it could not: re-reading pending here would throw away rows the
  // page was rendered with before the first paint settles.
  useEffect(() => {
    startRead(async () => {
      const settled = await readStatuses(SETTLED_STATUSES);
      // Set before the refusal check: a read that failed is still a read that
      // finished, and the alert below says why. Leaving the skeleton up would
      // spin forever on an error the operator can already see.
      setSettledRead(true);
      if (settled === null) return;
      setItems((prev) => [...prev.filter(isPending), ...settled].sort(byNewest));
    });
  }, [readStatuses]);

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
  // The server renders the PENDING page only, so an empty table before the
  // settled read lands is not an empty inbox — it is an unfinished one. Claiming
  // "No approvals yet" there is a statement nobody has checked yet, and on a
  // workspace whose approvals are all settled it flashes for as long as four
  // requests take before the rows arrive and replace it.
  //
  // Once the read is back the claim is the SERVER's, so it reads `items` rather
  // than the optimistic list: a row that has only just left keeps the table
  // silent rather than announcing a state the server has not confirmed.
  function emptyRegion(): ReactNode {
    if (!settledRead) {
      return <Skeleton className="h-24 w-full rounded-md" data-testid="approvals-loading" />;
    }
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
