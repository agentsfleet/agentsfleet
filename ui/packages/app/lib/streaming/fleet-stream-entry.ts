// The entry model behind the fleet-stream registry: one Entry per fleet, its
// published snapshot shape, and the factory that seeds a fresh entry from
// server-rendered rows. Data and construction only — the registry owns the
// map, the EventSource lifecycle, and every mutation. Split out for the same
// reason as `fleet-stream-frames.ts`: the registry's lifecycle file stays
// under the length gate and the model stays readable on its own.

import type { EventRow } from "@/lib/api/events";
import {
  NO_FACTS,
  latestFigures,
  type FleetFacts,
  type RunFigures,
} from "@/lib/events/run-summary";
import { maxServerCreatedAt, mergeBackfill } from "./fleet-stream-frames";
import type { FleetEvent } from "./fleet-stream-row";
import type { ReplyStream } from "./reply-stream";
import { CHAT_CONNECTION_STATUS, type ChatConnection, type ChatConnectionStatus } from "./chat-connection";
import { HeldTurns } from "./fleet-stream-held";

export const CONNECTION_STATUS = CHAT_CONNECTION_STATUS;
export type ConnectionStatus = ChatConnectionStatus;

export type FleetStreamSnapshot = {
  events: FleetEvent[];
  connectionStatus: ConnectionStatus;
  // What the server render and the live tail last said about the fleet
  // itself: its lifecycle status and how many approvals wait on it. The
  // console's summary strip reads these beside `latest`; both keep their
  // identity across frames that changed nothing they show.
  fleet: FleetFacts;
  // How many times a FRAME has moved `fleet` since the entry was created. A
  // server render pushes its own facts in through `reconcileServerFacts` and
  // never moves this; the hook that asked for that render compares it against
  // the value it saw when it asked, so a frame that landed while the render
  // was in flight is not rolled back by facts the server read before it.
  factsSeq: number;
  // The newest server row's figures, recomputed whenever the rows change and
  // kept by identity when the figures did not — so a subscriber selecting
  // `latest` does not re-render per chunk of a streaming reply.
  latest: RunFigures | null;
};

export type Listener = () => void;

export type Entry = {
  workspaceId: string;
  snapshot: FleetStreamSnapshot;
  replyStreams: Map<string, ReplyStream>;
  replyGaps: Set<string>;
  replyRecoveries: Set<string>;
  /** Per running event, when this tab last heard a frame for it. */
  replyHeard: Map<string, number>;
  /** Running events whose saved row is known to exist: a frame named it, or a
   * server read showed it. Only for these does a 404 mean the row is gone; an
   * event still queued behind its admission has no row yet. */
  replyExists: Set<string>;
  /** Settled events a backfill brought in without a body, whose saved row has
   * been read once for it. */
  bodyReads: Set<string>;
  /** Settled events whose turn closed a call it never heard end, whose saved
   * trace has been read once for it. */
  traceReads: Set<string>;
  listeners: Set<Listener>;
  refCount: number;
  eventSource: ChatConnection | null;
  idleTimer: ReturnType<typeof setTimeout> | null;
  // Newest server-confirmed created_at (epoch ms) — advanced only by the SSR
  // seed and successful backfill pages, never by client-clock-stamped live or
  // optimistic rows, and never by a failed backfill (so a failure cannot seal
  // the gap it left).
  serverSinceMs: number | null;
  backfillInFlight: boolean;
  // A recovery asked for while a walk was in flight: one more walk runs after it.
  backfillQueued: boolean;
  // A steer's frames waiting on a send's 202 to say which tab sent it.
  held: HeldTurns;
};

export const EMPTY_SNAPSHOT: FleetStreamSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,

  fleet: NO_FACTS,
  factsSeq: 0,
  latest: null,
}) as FleetStreamSnapshot;

export function createEntry(workspaceId: string, initial: EventRow[]): Entry {
  const events = mergeBackfill([], initial);
  return {
    workspaceId,
    snapshot: {
      events,
      connectionStatus: CONNECTION_STATUS.CONNECTING,

      fleet: NO_FACTS,
      factsSeq: 0,
      latest: latestFigures(events),
    },
    replyStreams: new Map(),
    replyGaps: new Set(),
    replyRecoveries: new Set(),
    replyHeard: new Map(),
    replyExists: new Set(),
    bodyReads: new Set(),
    traceReads: new Set(),
    listeners: new Set(),
    refCount: 0,
    eventSource: null,
    idleTimer: null,
    serverSinceMs: maxServerCreatedAt(null, initial),
    backfillInFlight: false,
    backfillQueued: false,
    held: new HeldTurns(),
  };
}
