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
} from "@usezombie/design-system";

import { useClientToken } from "@/lib/auth/client";
import {
  approveApproval,
  denyApproval,
  listApprovals,
  type ApprovalGate,
  type ResolveOutcome,
} from "@/lib/api/approvals";

const POLL_MS = 5000;

type Props = {
  workspaceId: string;
  initialItems: ApprovalGate[];
  initialCursor: string | null;
  /** When set, the list is filtered server-side by this zombie. */
  zombieId?: string;
};

export default function ApprovalsList({ workspaceId, initialItems, initialCursor, zombieId }: Props) {
  const { getToken } = useClientToken();
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
        g.zombie_name.toLowerCase().includes(q) ||
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
  // Skip the poll-driven reset once the operator has clicked Load more.
  // Polling fetches page 1 only (`limit: 50`, no cursor); replacing items
  // wholesale would silently drop the loaded-more pages. A ref is fine —
  // the latest value is read inside the interval callback, no re-render
  // needed.
  const hasLoadedMore = useRef(false);
  useEffect(() => {
    let alive = true;
    const id = setInterval(async () => {
      if (hasLoadedMore.current) return;
      const token = await getToken();
      if (!token || !alive) return;
      try {
        const next = await listApprovals(workspaceId, token, {
          limit: 50,
          zombieId,
        });
        if (!alive || hasLoadedMore.current) return;
        setItems(next.items);
        setCursor(next.next_cursor);
      } catch {
        // Transient — leave the existing list rendered until the next tick.
      }
    }, POLL_MS);
    return () => {
      alive = false;
      clearInterval(id);
    };
  }, [workspaceId, zombieId, getToken]);

  function loadMore() {
    if (!cursor) return;
    setError(null);
    startTransition(async () => {
      const token = await getToken();
      if (!token) {
        setError("Not authenticated");
        return;
      }
      try {
        const next = await listApprovals(workspaceId, token, {
          cursor,
          zombieId,
          limit: 50,
        });
        setItems((prev) => [...prev, ...next.items]);
        setCursor(next.next_cursor);
        // Latch the polling guard so the next 5s tick doesn't reset the
        // operator back to page 1 by replacing items with the first page.
        hasLoadedMore.current = true;
      } catch (e) {
        setError((e as Error).message ?? "Failed to load more");
      }
    });
  }

  async function resolve(gateId: string, decision: "approve" | "deny") {
    setError(null);
    const token = await getToken();
    if (!token) {
      setError("Not authenticated");
      return;
    }
    const fn = decision === "approve" ? approveApproval : denyApproval;
    try {
      const outcome: ResolveOutcome = await fn(workspaceId, gateId, token);
      // Optimistic removal — even on already_resolved the row leaves the
      // pending list. Toasts could be added later; for v1 the list update
      // alone is the operator-visible signal.
      setItems((prev) => prev.filter((g) => g.gate_id !== gateId));
      if (outcome.kind === "already_resolved") {
        setError(
          `Already ${outcome.data.outcome} by ${outcome.data.resolved_by}`,
        );
      }
    } catch (e) {
      setError((e as Error).message ?? "Resolve failed");
    }
  }

  if (filtered.length === 0 && filter.trim() === "" && !error) {
    return (
      <EmptyState title="No pending approvals" description="Nothing waiting on operator action." />
    );
  }

  return (
    <>
      <div className="mb-4">
        <Input
          type="search"
          placeholder="Filter by zombie, tool, or action…"
          value={filter}
          onChange={(e) => setFilter(e.currentTarget.value)}
          aria-label="Filter approvals"
        />
      </div>

      <ul className="space-y-3">
        {filtered.map((g) => (
          <li key={g.gate_id}>
            <ApprovalCard gate={g} onResolve={resolve} />
          </li>
        ))}
      </ul>

      {error ? (
        <Alert variant="destructive" className="mt-3">{error}</Alert>
      ) : null}

      {cursor ? (
        <div className="mt-4 flex justify-center">
          <Button variant="ghost" size="sm" onClick={loadMore} disabled={pending} aria-busy={pending}>
            {pending ? "Loading…" : "Load more"}
          </Button>
        </div>
      ) : null}
    </>
  );
}

function ApprovalCard({
  gate,
  onResolve,
}: {
  gate: ApprovalGate;
  onResolve: (gateId: string, decision: "approve" | "deny") => Promise<void>;
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
              <Link href={`/approvals/${gate.gate_id}`} className="hover:underline">
                {gate.proposed_action || `${gate.tool_name}:${gate.action_name}`}
              </Link>
            </CardTitle>
            <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
              <Link href={`/zombies/${gate.zombie_id}`} className="font-medium hover:underline">
                {gate.zombie_name}
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
        <Button size="sm" onClick={() => void onResolve(gate.gate_id, "approve")}>
          Approve
        </Button>
        <Button size="sm" variant="destructive" onClick={() => void onResolve(gate.gate_id, "deny")}>
          Deny
        </Button>
        <Button asChild size="sm" variant="ghost">
          <Link href={`/approvals/${gate.gate_id}`}>Details</Link>
        </Button>
      </CardFooter>
    </Card>
  );
}
