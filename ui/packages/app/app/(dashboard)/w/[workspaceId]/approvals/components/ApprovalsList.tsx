"use client";

import { useEffect, useMemo, useRef, useState, useTransition } from "react";
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
  type ApprovalDecision,
  type ApprovalGate,
  type ResolveOutcome,
} from "@/lib/api/approvals";
import { workspacePath } from "@/lib/workspace-routes";
import { presentErrorString } from "@/lib/errors";
import { deriveFleetIdentity } from "../../fleets/components/fleetIdentity";

const POLL_MS = 5000;
const AGENT_PREFIX = "Agent";

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

  const filtered = useMemo(() => {
    const q = filter.trim().toLowerCase();
    if (!q) return items;
    return items.filter(
      (g) =>
        g.fleet_name.toLowerCase().includes(q) ||
        `${AGENT_PREFIX} ${deriveFleetIdentity(g.fleet_id).callsign}`.toLowerCase().includes(q) ||
        g.tool_name.toLowerCase().includes(q) ||
        g.action_name.toLowerCase().includes(q) ||
        g.gate_kind.toLowerCase().includes(q) ||
        g.proposed_action.toLowerCase().includes(q),
    );
  }, [items, filter]);

  // Background poll. SWR not yet on this page, so a manual interval keeps
  // the list within ~5 s of reality. Worker wake on resolution is a separate
  // ≤2 s concern handled server-side.
  //
  // Skip the poll-driven reset once the human has clicked Load more.
  // Polling fetches page 1 only (`limit: 50`, no cursor); replacing items
  // wholesale would silently drop the loaded-more pages. A ref is fine —
  // the latest value is read inside the interval callback, no re-render
  // needed.
  const hasLoadedMore = useRef(false);
  useEffect(() => {
    let alive = true;
    // Read through an accessor so the post-await re-check isn't narrowed away:
    // `loadMore` can flip the ref to true during the in-flight fetch.
    const alreadyPaged = () => hasLoadedMore.current;
    const tick = async () => {
      if (alreadyPaged()) return;
      const result = await listApprovalsAction(workspaceId, { limit: 50, fleetId });
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
    };
    const id = setInterval(() => { void tick(); }, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [workspaceId, fleetId]);

  // `cursor` is passed in (narrowed to a non-null string by the `{cursor ? …}`
  // render guard on the trigger), so no in-function null check is needed.
  function loadMore(cursor: string) {
    setError(null);
    startTransition(async () => {
      const result = await listApprovalsAction(workspaceId, { cursor, fleetId, limit: 50 });
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

  async function resolve(gateId: string, decision: ApprovalDecision) {
    setError(null);
    const isApprove = decision === APPROVAL_DECISION.APPROVE;
    const action = isApprove ? approveApprovalAction : denyApprovalAction;
    const result = await action(workspaceId, gateId);
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
    // Optimistic removal — even on already_resolved the row leaves the
    // pending list. Toasts could be added later; for v1 the list update
    // alone is the human-visible signal.
    setItems((prev) => prev.filter((g) => g.gate_id !== gateId));
    if (outcome.kind === "already_resolved") {
      setError(
        `Already ${outcome.data.outcome} by ${outcome.data.resolved_by}`,
      );
    }
  }

  if (filtered.length === 0 && filter.trim() === "" && !error) {
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
  onResolve: (gateId: string, decision: ApprovalDecision) => Promise<void>;
}) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 30_000);
    return () => clearInterval(id);
  }, []);
  const ageMin = Math.max(0, Math.floor((now - gate.requested_at) / 60_000));
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
        <Button size="sm" onClick={() => void onResolve(gate.gate_id, APPROVAL_DECISION.APPROVE)}>
          Approve
        </Button>
        <Button
          size="sm"
          variant="destructive"
          onClick={() => void onResolve(gate.gate_id, APPROVAL_DECISION.DENY)}
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
