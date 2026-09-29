import type { FleetCountersSnapshot, WorkspaceLiveFrame } from "@/lib/api/events";
import type { ConnectionStatus } from "@/lib/streaming/fleet-stream-registry";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";

import { FRAME_KIND } from "@/lib/api/events-types";

/**
 * The tile footer: where the fleet stands, as the last frame said.
 *
 * Server truth, assigned from whichever frame arrived last and never added
 * to. `undefined` until the stream has said anything about the fleet, so the
 * tile keeps rendering the server-rendered figures rather than a zero.
 */
export type TileCounters = {
  spentNanos: number;
  eventsProcessed: number;
};

/**
 * Everything a wall tile shows of the stream, and nothing else. A tile reads
 * the newest row's trigger text, never its reply or its tool calls, so a
 * snapshot that carried the rows would change on every chunk of every reply
 * and repaint a tile that looks the same.
 */
export type WorkspaceTileSnapshot = {
  feed: string | undefined;
  connectionStatus: ConnectionStatus;
  helloReceived: boolean;
  isLive: boolean;
  catchingUp: boolean;
  counters: TileCounters | undefined;
};

type MidRunFrame = Extract<
  WorkspaceLiveFrame,
  {
    kind:
      | typeof FRAME_KIND.CHUNK
      | typeof FRAME_KIND.TOOL_CALL_STARTED
      | typeof FRAME_KIND.TOOL_CALL_PROGRESS
      | typeof FRAME_KIND.TOOL_CALL_COMPLETED;
  }
>;

const MID_RUN_KINDS: ReadonlySet<string> = new Set([
  FRAME_KIND.CHUNK,
  FRAME_KIND.TOOL_CALL_STARTED,
  FRAME_KIND.TOOL_CALL_PROGRESS,
  FRAME_KIND.TOOL_CALL_COMPLETED,
]);

/** A frame that only adds to a row's reply or its tool calls — the bulk of a busy stream. */
export function isMidRunFrame(frame: WorkspaceLiveFrame): frame is MidRunFrame {
  return MID_RUN_KINDS.has(frame.kind);
}

/** The newest row's trigger text, which is the whole of a tile's feed line. */
export function feedOf(events: readonly FleetEvent[] | undefined): string | undefined {
  return events?.at(-1)?.text;
}

/** Whether two projections render the same tile. Counters compare by identity: `mergeCounters` keeps the standing object when the figures did not move. */
export function sameTile(a: WorkspaceTileSnapshot, b: WorkspaceTileSnapshot): boolean {
  return (
    a.feed === b.feed &&
    a.counters === b.counters &&
    a.connectionStatus === b.connectionStatus &&
    a.helloReceived === b.helloReceived &&
    a.isLive === b.isLive &&
    a.catchingUp === b.catchingUp
  );
}

/**
 * A counter as the daemon writes one: a whole, non-negative number a JSON
 * payload can carry exactly. Anything else is not a counter, whatever the
 * frame calls it.
 */
function isCounter(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

/**
 * The snapshot a frame carries, or nothing. Both fields or neither: the
 * daemon flattens an optional pair, so a frame with one number and not the
 * other is malformed and is treated as carrying none — the figures already
 * standing stay, and nothing is guessed. The wire is untrusted, so the carrier
 * itself is checked before it is read: a hello whose map holds `null` for a
 * fleet must not throw halfway through applying the greeting.
 */
function readCounters(carried: unknown): TileCounters | undefined {
  if (typeof carried !== "object" || carried === null) return undefined;
  const { events_processed: eventsProcessed, budget_used_nanos: spentNanos } =
    carried as FleetCountersSnapshot;
  if (!isCounter(eventsProcessed) || !isCounter(spentNanos)) return undefined;
  return { eventsProcessed, spentNanos };
}

/**
 * ASSIGNED, never added to. Every daemon frame carries the whole truth, so
 * the same frame twice leaves the tile where it was — the reason no frame
 * owns an increment. Both counters only ever grow on the server, so what is
 * kept is the GREATER of the standing figure and the carried one: a frame
 * that crossed a `hello` in flight, or two publishers whose reads and
 * publishes interleaved, cannot walk a tile backwards. The standing object
 * itself is returned when neither figure moved, so its tile keeps its
 * snapshot.
 */
export function mergeCounters(
  standing: TileCounters | undefined,
  carried: unknown,
): TileCounters | undefined {
  const counters = readCounters(carried);
  if (counters === undefined) return standing;
  if (standing === undefined) return counters;
  const eventsProcessed = Math.max(standing.eventsProcessed, counters.eventsProcessed);
  const spentNanos = Math.max(standing.spentNanos, counters.spentNanos);
  if (eventsProcessed === standing.eventsProcessed && spentNanos === standing.spentNanos) {
    return standing;
  }
  return { eventsProcessed, spentNanos };
}
