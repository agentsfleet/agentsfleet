import type { EventRow } from "@/lib/api/events";

// The figures the chat's metrics strip shows, and the one builder both sides
// derive them with: the server's first render (from the thread page it already
// fetched) and the completion refresh (from the newest-event read). Pure and
// dependency-free on purpose — it is imported by a Server Component, a Server
// Action, and a client leaf alike.

/** Pending approvals the strip counts for one fleet before it says "and more". */
export const RUN_SUMMARY_APPROVALS_LIMIT = 50;
/** The strip reads only the newest event row; one is the whole ask. */
export const RUN_SUMMARY_LATEST_LIMIT = 1;

export type FleetRunSummary = {
  /** The fleet's lifecycle status as the server last reported it. */
  status: string;
  /** The newest event row, or null when the fleet has none yet. */
  latest: EventRow | null;
  /** False when the read that would have carried `latest` failed. */
  latestAvailable: boolean;
  pendingApprovals: number;
  pendingApprovalsHasMore: boolean;
  /** False when the approvals read failed — distinct from "none pending". */
  approvalsAvailable: boolean;
};

/** Any page of rows, newest first. The thread page qualifies: its rows carry
 * bodies the strip ignores. */
type RowsPage = { items: readonly EventRow[] } | null;
type ApprovalsPage = { items: readonly unknown[]; next_cursor: string | null } | null;

/**
 * A null page means the read failed, which the strip reports as unavailable;
 * an empty page means the fleet has nothing yet, which it reports as empty.
 * The two must never collapse into one another.
 */
export function buildRunSummary(
  status: string,
  rows: RowsPage,
  approvals: ApprovalsPage,
): FleetRunSummary {
  return {
    status,
    latest: rows?.items[0] ?? null,
    latestAvailable: rows !== null,
    pendingApprovals: approvals?.items.length ?? 0,
    pendingApprovalsHasMore: approvals !== null && approvals.next_cursor !== null,
    approvalsAvailable: approvals !== null,
  };
}
