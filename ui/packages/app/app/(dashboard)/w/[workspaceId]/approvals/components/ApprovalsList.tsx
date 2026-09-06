"use client";

import { useEffect, useMemo, useOptimistic, useRef, useState, useTransition } from "react";
import Link from "next/link";
import {
  Alert,
  Badge,
  Button,
  Card,
  CardContent,
  CardFooter,
  CardHeader,
  CardTitle,
  EmptyState,
  Input,
  List,
  ListItem,
} from "@agentsfleet/design-system";
import { CheckCircle2Icon } from "lucide-react";

import {
  approveApprovalAction,
  denyApprovalAction,
  listApprovalsAction,
} from "../actions";
import {
  APPROVAL_DECISION,
  APPROVALS_PAGE_LIMIT,
  type ApprovalDecision,
  type ApprovalGate,
  type ResolveOutcome,
} from "@/lib/api/approvals";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";
import type { ActionResult } from "@/lib/actions/with-token";
import { deriveFleetIdentity } from "../../fleets/components/fleetIdentity";

const POLL_MS = 5000;
const AGENT_PREFIX = "Agent";

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

export default function ApprovalsList({ workspaceId, initialItems, initialCursor, fleetId }: Props) {
  const [items, setItems] = useState<ApprovalGate[]>(initialItems);
  const [cursor, setCursor] = useState<string | null>(initialCursor);
  const [filter, setFilter] = useState<string>("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  // Separate from the load-more transition: `pending` disables pagination, and
  // a resolve in flight must not grey out the Load more button.
  const [, startResolve] = useTransition();
  // A resolved row leaves the inbox at the click, not at the answer. The base
  // list is updated on success; a failed resolve ends the transition and the
  // row comes back from that base on its own.
  const [visibleItems, hideGate] = useOptimistic(
    items,
    (current: ApprovalGate[], gateId: string) => current.filter((g) => g.gate_id !== gateId),
  );

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return visibleItems;
    return visibleItems.filter(
      (g) =>
        g.fleet_name.toLowerCase().includes(q) ||
        `${AGENT_PREFIX} ${deriveFleetIdentity(g.fleet_id).callsign}`.toLowerCase().includes(q) ||
        g.tool_name.toLowerCase().includes(q) ||
        g.action_name.toLowerCase().includes(q) ||
        g.gate_kind.toLowerCase().includes(q) ||
        g.proposed_action.toLowerCase().includes(q),
    );
  }, [visibleItems, filter]);

  // Background poll. SWR not yet on this page, so a manual interval keeps
  // the list within ~5 s of reality. Worker wake on resolution is a separate
  // ≤2 s concern handled server-side.
  //
  // Skip the poll-driven reset once the human has clicked Load more.
  // Polling fetches page 1 only (`APPROVALS_PAGE_LIMIT`, no cursor); replacing items
  // wholesale would silently drop the loaded-more pages. A ref is fine —
  // the latest value is read inside the interval callback, no re-render
  // needed.
  const hasLoadedMore = useRef(false);
  useEffect(() => {
    let alive = true;
    // One read on the wire at a time. A slow backend answers a tick after the
    // next has fired; without this latch the ticks stack, each retrying on its
    // own, and one open inbox multiplies the load on a backend already behind.
    // A tick that skips is not lost — the next one reads the same page.
    let inFlight = false;
    // Read through an accessor so the post-await re-check isn't narrowed away:
    // `loadMore` can flip the ref to true during the in-flight fetch.
    const alreadyPaged = () => hasLoadedMore.current;
    // A tab nobody is looking at asks for nothing: the read it would make is
    // thrown away unseen, and a backend already behind is the one that pays.
    // The moment the tab is looked at again, one read catches the list up.
    const hidden = () => document.visibilityState === "hidden";
    const tick = async () => {
      if (alreadyPaged() || inFlight || hidden()) return;
      inFlight = true;
      try {
        const result = await listApprovalsAction(workspaceId, { limit: APPROVALS_PAGE_LIMIT, fleetId });
        if (!alive || alreadyPaged()) return;
        if (!result.ok) {
          // 401 is terminal — silently retrying for 5s forever leaves the
          // human staring at a stale list with no signal that their
          // session expired. Surface it; refresh fixes it.
          if (result.status === 401) {
            setError("Session expired — refresh the page to sign back in.");
            return;
          }
          // Transient (5xx, network blips, etc.) — leave the existing list
          // rendered until the next tick.
          return;
        }
        setItems(result.data.items);
        setCursor(result.data.next_cursor);
      } finally {
        inFlight = false;
      }
    };
    const id = setInterval(() => { void tick(); }, POLL_MS);
    const onVisible = () => {
      if (!hidden()) void tick();
    };
    document.addEventListener("visibilitychange", onVisible);
    return () => {
      alive = false;
      clearInterval(id);
      document.removeEventListener("visibilitychange", onVisible);
    };
  }, [workspaceId, fleetId]);

  // `cursor` is passed in (narrowed to a non-null string by the `{cursor ? …}`
  // render guard on the trigger), so no in-function null check is needed.
  function loadMore(cursor: string) {
    setError(null);
    startTransition(async () => {
      const result = await listApprovalsAction(workspaceId, { cursor, fleetId, limit: APPROVALS_PAGE_LIMIT });
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: "load more approvals",
          }),
        );
        return;
      }
      setItems((prev) => [...prev, ...result.data.items]);
      setCursor(result.data.next_cursor);
      // Latch the polling guard so the next 5s tick doesn't reset the
      // human back to page 1 by replacing items with the first page.
      hasLoadedMore.current = true;
    });
  }

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
      // Gone for good either way: resolved here, or already resolved elsewhere —
      // the pending inbox has no row for it in both cases.
      setItems((prev) => prev.filter((g) => g.gate_id !== gateId));
      if (outcome.kind === "already_resolved") {
        setError(`Already ${outcome.data.outcome} by ${outcome.data.resolved_by}`);
      }
    });
  }

  // The empty state is a claim about the server's inbox, so it reads the
  // confirmed list: a row that has only optimistically left keeps the list
  // shell (input, container) in place until the resolve is answered, and an
  // aria-live "nothing waiting" is never announced ahead of the server.
  if (items.length === 0 && filter.trim() === "" && !error) {
    return (
      <EmptyState
        icon={<CheckCircle2Icon size={28} />}
        title="No pending approvals"
        description="Nothing waiting on human review."
      />
    );
  }

  return (
    <>
      <div className="mb-4">
        <Input
          type="search"
          placeholder="Filter by fleet, tool, or action…"
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
          aria-label="Filter approvals"
        />
      </div>

      <List variant="plain" className="space-y-3">
        {filtered.map((g) => (
          <ListItem key={g.gate_id}>
            <ApprovalCard gate={g} workspaceId={workspaceId} onResolve={resolve} />
          </ListItem>
        ))}
      </List>

      {error ? (
        <Alert variant="destructive" className="mt-3">{error}</Alert>
      ) : null}

      {cursor ? (
        <div className="mt-4 flex justify-center">
          <Button variant="ghost" size="sm" onClick={() => loadMore(cursor)} disabled={pending} aria-busy={pending}>
            {pending ? "Loading…" : "Load more"}
          </Button>
        </div>
      ) : null}
    </>
  );
}

function ApprovalCard({
  gate,
  workspaceId,
  onResolve,
}: {
  gate: ApprovalGate;
  workspaceId: string;
  onResolve: (gateId: string, decision: ApprovalDecision) => void;
}) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);
  const ageMin = Math.max(0, Math.floor((now - gate.created_at) / 60_000));
  const timeoutMin = Math.max(0, Math.ceil((gate.timeout_at - now) / 60_000));
  return (
    <Card>
      <CardHeader>
        <div className="flex items-start justify-between gap-3">
          <div className="flex flex-col gap-1">
            <CardTitle className="text-base">
              <Link href={workspacePath(workspaceId, `approvals/${gate.gate_id}`)} className="hover:underline">
                {gate.proposed_action || `${gate.tool_name}:${gate.action_name}`}
              </Link>
            </CardTitle>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <Link href={workspacePath(workspaceId, `fleets/${gate.fleet_id}`)} className="font-medium hover:underline">
                {`${AGENT_PREFIX} ${deriveFleetIdentity(gate.fleet_id).callsign}`}
              </Link>
              {gate.gate_kind ? <Badge variant="default">{gate.gate_kind}</Badge> : null}
              <span>requested {ageMin}m ago</span>
              <span>auto-deny in {timeoutMin}m</span>
            </div>
          </div>
        </div>
      </CardHeader>
      {gate.blast_radius ? (
        <CardContent>
          <p className="text-sm">{gate.blast_radius}</p>
        </CardContent>
      ) : null}
      <CardFooter className="gap-2">
        <Button size="sm" onClick={() => onResolve(gate.gate_id, APPROVAL_DECISION.APPROVE)}>
          Approve
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onClick={() => onResolve(gate.gate_id, APPROVAL_DECISION.DENY)}
        >
          Deny
        </Button>
        <Button asChild size="sm" variant="ghost">
          <Link href={workspacePath(workspaceId, `approvals/${gate.gate_id}`)}>Details</Link>
        </Button>
      </CardFooter>
    </Card>
  );
}
