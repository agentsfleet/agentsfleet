import type { EventRow } from "@/lib/api/events";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { EVENT_STATUS } from "./event-summary";

// The figures the chat's metrics strip shows, and the builders every side
// derives them with: the server's first render (from the thread page it
// already fetched), the stream registry (from the rows and frames it holds),
// and the live-tail frames the daemon publishes. Pure and dependency-light on
// purpose — it is imported by a Server Component, the streaming registry, and
// a client leaf alike.

// The fields the strip reads off one run, named once: the type, the pick off
// a row, and the field-wise comparison are all driven from this list, so a
// field added here reaches every one of them or none.
const RUN_FIGURE_KEYS = [
  "status",
  "failure_label",
  "failure_detail",
  "created_at",
  "tokens",
  "wall_ms",
  "cost_nanos",
] as const;

/** What the strip reads off one run: the list row's outcome and figures. */
export type RunFigures = Pick<EventRow, (typeof RUN_FIGURE_KEYS)[number]>;

export type FleetRunSummary = {
  /** The fleet's lifecycle status as the server last reported it. */
  status: string;
  /** The newest run's figures, or null when the fleet has none yet. */
  latest: RunFigures | null;
  /** False when the read that would have carried `latest` failed. */
  latestAvailable: boolean;
  /** How many approvals wait on this fleet, as the server last counted. */
  pendingApprovals: number;
};

/**
 * What the live tail has said about the fleet itself, apart from its rows.
 * Null until a server render or a frame says; the strip falls back to the
 * server's figures for whichever fact is still null.
 */
export type FleetFacts = {
  status: string | null;
  pendingApprovals: number | null;
};

export const NO_FACTS: FleetFacts = Object.freeze({
  status: null,
  pendingApprovals: null,
}) as FleetFacts;

/** Any page of rows, newest first. The thread page qualifies: its rows carry
 * bodies the strip ignores. */
type RowsPage = { items: readonly EventRow[] } | null;

/**
 * A null page means the read failed, which the strip reports as unavailable;
 * an empty page means the fleet has nothing yet, which it reports as empty.
 * The two must never collapse into one another.
 */
export function buildRunSummary(
  status: string,
  rows: RowsPage,
  pendingApprovals: number,
): FleetRunSummary {
  const newest = rows?.items[0];
  return {
    status,
    latest: newest === undefined ? null : figuresOfRow(newest),
    latestAvailable: rows !== null,
    pendingApprovals,
  };
}

/** The strip's view of a list row — only the fields it renders. */
export function figuresOfRow(row: EventRow): RunFigures {
  return Object.fromEntries(RUN_FIGURE_KEYS.map((key) => [key, row[key]])) as RunFigures;
}

// The statuses the server writes. A row the browser made — an optimistic
// steer awaiting its identifier, a send the server refused — carries the
// composer's vocabulary instead, and is not the fleet's latest run: the server
// never saw it.
const SERVER_STATUSES: ReadonlySet<string> = new Set(Object.values(EVENT_STATUS));

/**
 * The newest server row's figures, as the strip shows them, or null when the
 * timeline holds no server row. Newest by the row's own instant, then by the
 * stream entry id — the order the events list serves, so the strip and the
 * server's first render agree on which row is latest.
 */
export function latestFigures(events: readonly FleetEvent[]): RunFigures | null {
  let newest: FleetEvent | undefined;
  for (const event of events) {
    if (!SERVER_STATUSES.has(event.status)) continue;
    if (newest === undefined || isNewer(event, newest)) newest = event;
  }
  if (newest === undefined) return null;
  return {
    status: newest.status,
    failure_label: newest.failureLabel,
    failure_detail: newest.failureDetail,
    created_at: newest.createdAt.getTime(),
    tokens: newest.tokens ?? null,
    wall_ms: newest.wallMs ?? null,
    cost_nanos: newest.costNanos ?? null,
  };
}

function isNewer(candidate: FleetEvent, incumbent: FleetEvent): boolean {
  const at = candidate.createdAt.getTime();
  const than = incumbent.createdAt.getTime();
  return at !== than ? at > than : candidate.id > incumbent.id;
}

/** Field-wise equality, so a snapshot keeps its figures' identity across frames
 * that changed nothing the strip shows. */
export function sameFigures(a: RunFigures | null, b: RunFigures | null): boolean {
  if (a === null || b === null) return a === b;
  return RUN_FIGURE_KEYS.every((key) => a[key] === b[key]);
}
