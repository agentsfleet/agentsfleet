import { AGENTSFLEET_STATUS } from "@/lib/api/fleets";
import { NANOS_PER_USD } from "@/lib/types";
import type { ConnectionStatus } from "@/lib/streaming/fleet-stream-registry";
import { CONNECTION_STATUS } from "@/lib/streaming/fleet-stream-registry";

// A wall tile is exactly one of these — never a blank. The tagged union is the
// enforcement: a stream refusal or error maps to `snapshot`, and the tile's
// exhaustive switch has no fourth arm to fall into (Invariant 1, "no dead
// tile"). `snapshot` is visually indistinguishable from `live` except its
// eyebrow, so an operator whose stream budget is spent still sees real recent
// activity rather than an empty card.
export type TileKind = "live" | "drained" | "snapshot";

export type TileLiveness =
  | { kind: "drained" }
  | { kind: "live" }
  | { kind: "snapshot"; reason: SnapshotReason };

// The one snapshot reason the wall distinguishes. A 503/ERR_SSE_STREAM_CAP
// refusal and any mid-flight stream error are collapsed here on purpose: the
// operator-visible outcome is identical (no live feed), and one fallback path
// is one thing to test, not two (spec §2 implementation default). The browser
// EventSource cannot read the 503 body or headers, so the client cannot tell a
// cap-refusal from a network drop anyway — both surface as a persistent
// reconnect, which is what this reason names.
export const SNAPSHOT_CAPPED_OR_ERRORED = "capped_or_errored" as const;
export type SnapshotReason = typeof SNAPSHOT_CAPPED_OR_ERRORED;

const DRAINED_STATUSES: ReadonlySet<string> = new Set([
  AGENTSFLEET_STATUS.STOPPED,
  AGENTSFLEET_STATUS.PAUSED,
  AGENTSFLEET_STATUS.KILLED,
]);

// Pure liveness derivation — the single source both the tile and its tests
// read. A parked or killed fleet is `drained` and opens no stream at all; a
// live fleet whose connection is up (or still opening) is `live`; a live fleet
// whose connection is stuck reconnecting shows its last event as a `snapshot`
// rather than going dark. The registry reconnects forever with backoff, so
// `reconnecting` is exactly "the feed is gone right now" — the honest snapshot
// trigger — and if it recovers this flips back to `live` on the next frame.
export function deriveTileLiveness(
  status: string,
  connectionStatus: ConnectionStatus,
): TileLiveness {
  if (DRAINED_STATUSES.has(status)) return { kind: "drained" };
  if (connectionStatus === CONNECTION_STATUS.RECONNECTING) {
    return { kind: "snapshot", reason: SNAPSHOT_CAPPED_OR_ERRORED };
  }
  return { kind: "live" };
}

// True when a fleet's tile should open its own stream. Drained fleets never do
// (Dimension 1.3) — this is what the tile checks before choosing which subtree
// to render, keeping the streaming hook out of the drained path entirely rather
// than subscribing and ignoring the result.
export function tileShouldStream(status: string): boolean {
  return !DRAINED_STATUSES.has(status);
}

// Server-truth spend, formatted for the tile footer. Reads the summed
// `credit_deducted_nanos` the row already carries — never token×rate math on
// the client (Invariant 2). A fleet an older daemon served without the field
// renders a dash, not a misleading `$0.00`.
export function formatTileSpend(budgetUsedNanos: number | undefined): string {
  if (budgetUsedNanos == null) return "—";
  return `$${(budgetUsedNanos / NANOS_PER_USD).toFixed(2)}`;
}

// Lifetime event count for the tile footer. Undefined (older daemon) renders a
// dash so a real zero and a missing field never look the same.
export function formatTileEvents(eventsProcessed: number | undefined): string {
  if (eventsProcessed == null) return "—";
  return String(eventsProcessed);
}
