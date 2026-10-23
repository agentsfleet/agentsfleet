import { type EventRow } from "@/lib/api/events";
import { FRAME_KIND, ROUTED_FRAME_KINDS, streamFleetEventsUrl } from "@/lib/api/events-types";
import type { FleetFacts } from "@/lib/events/run-summary";
import { landFrames, onFrame, recoverGap, type LiveEntry } from "./fleet-stream-dispatch";
import { mergeFacts } from "./fleet-stream-facts";
import { optimisticRow, reconcileRows } from "./fleet-stream-optimistic";
import { patchSnapshot, setEvents } from "./fleet-stream-snapshot";
import { ChatConnection } from "./chat-connection";
import { mergeBackfill, parseLiveFrame } from "./fleet-stream-frames";
import {
  disposeReplyStreams,
  markReplyGap,
  readStalledReplies,
  watchReply,
  watchRunningRows,
} from "./fleet-stream-reply-registry";
import {
  CONNECTION_STATUS,
  EMPTY_SNAPSHOT,
  createEntry,
  type Entry,
  type FleetStreamSnapshot,
  type Listener,
} from "./fleet-stream-entry";
export {
  CONNECTION_STATUS,
  type ConnectionStatus,
  type FleetStreamSnapshot,
} from "./fleet-stream-entry";

// Identity-owned subscription registry. One Entry per fleetId; multiple
// React hook instances share it via refcounted subscribe/release. The
// EventSource survives a /dashboard ↔ /fleets/[id] round-trip up to
// IDLE_RELEASE_MS after the last consumer detaches — anything longer
// and we tear down so a never-revisited tab doesn't leak a connection.
//
// The initial event list is seeded from server-rendered data passed by
// the caller (no client-side backfill GET, no bearer token in the
// browser); live updates ride the cookie-authed SSE route handler. A
// reconnect open — never the initial one — additionally backfills the
// frames published during the outage via the same-origin events proxy,
// merged through the id-deduping mergeBackfill.


const IDLE_RELEASE_MS = 30_000;

// Module-level, not per-entry: a pending send's optimistic row deliberately
// outlives the stream entry, which is torn down after the idle window and
// recreated with fresh state. A per-entry counter restarting at 1 would let a
// stale tempId collide with a new row's id — and a refusal's discard would
// then remove the operator's newest pending message.
let tempCounter = 0;

/** One mounted account, workspace and subject own this registry. */
export class FleetStreams {
  readonly #entries = new Map<string, LiveEntry>();

private adopt(workspaceId: string, fleetId: string, initial: EventRow[]): LiveEntry {
  const entry: LiveEntry = {
    ...createEntry(workspaceId, initial),
    apply: (next, facts) => setEvents(entry, next, facts),
    isCurrent: () => this.#entries.get(fleetId) === entry,
  };
  watchRunningRows(entry, initial);
  this.#entries.set(fleetId, entry);
  return entry;
}

private connect(entry: LiveEntry, fleetId: string): void {
  const sweep = (): void => readStalledReplies(entry, fleetId, entry.apply, entry.isCurrent);
  const connection = new ChatConnection({
    url: streamFleetEventsUrl(entry.workspaceId, fleetId),
    events: [...ROUTED_FRAME_KINDS, FRAME_KIND.ACCESS_REVOKED],
    onOpen: (recovered) => { if (recovered) recoverGap(entry, fleetId); },
    onInterrupted: () => markReplyGap(entry),
    onHeartbeat: sweep,
    onStatus: (connectionStatus) => {
      if (entry.snapshot.connectionStatus !== connectionStatus) patchSnapshot(entry, {connectionStatus});
    },
    onFrame: (name, data) => {
      if (name === FRAME_KIND.ACCESS_REVOKED) {
        connection.close(); entry.eventSource = null;
        disposeReplyStreams(entry);
        patchSnapshot(entry, {connectionStatus: CONNECTION_STATUS.REVOKED});
        return false;
      }
      const frame = parseLiveFrame(data);
      if (!frame) return false;
      onFrame(entry, fleetId, frame); sweep();
      return true;
    },
  });
  entry.eventSource = connection;
  connection.start();
}

retryConnection(fleetId: string): void {
  const entry = this.#entries.get(fleetId);
  if (!entry || entry.snapshot.connectionStatus === CONNECTION_STATUS.REVOKED) return;
  entry.eventSource?.retry();
}

private teardown(entry: Entry, fleetId: string): void {
  disposeReplyStreams(entry);
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  entry.eventSource?.close(); entry.eventSource = null;
  this.#entries.delete(fleetId);
}

subscribe(
  workspaceId: string,
  fleetId: string,
  initial: EventRow[],
  listener: Listener,
): () => void {
  let entry = this.#entries.get(fleetId);
  if (!entry) {
    entry = this.adopt(workspaceId, fleetId, initial);
    this.connect(entry, fleetId);
  } else if (entry.snapshot.connectionStatus === CONNECTION_STATUS.REVOKED) {
    // A new mount is a new request: access may have come back since the stream
    // ended, by a fresh invite. A caller still removed is refused once more
    // with `access_revoked`, so this costs one request per mount, never a loop.
    patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.CONNECTING });
    this.connect(entry, fleetId);
  }
  if (entry.idleTimer) {
    clearTimeout(entry.idleTimer);
    entry.idleTimer = null;
  }
  entry.refCount += 1;
  entry.listeners.add(listener);
  const subscribed = entry;
  return () => {if (subscribed.isCurrent()) this.releaseSubscriber(subscribed, fleetId, listener);};
}

reconcileServerRows(fleetId: string, rows: EventRow[]): void {
  const entry = this.#entries.get(fleetId);
  if (!entry || rows.length === 0) return;
  setEvents(entry, (prev) => mergeBackfill(prev, rows));
  watchRunningRows(entry, rows);
}

// A server render's word on the fleet, as the page just read it. It overwrites
// what the tail last said — a kill from the header reaches the strip this way,
// since no frame announces a PATCH — and the next frame overwrites it back.
// Whether a render is older than a frame that landed while it was in flight
// is the caller's to decide, from `factsSeq`; `factsSeq` never moves here.
reconcileServerFacts(fleetId: string, facts: FleetFacts): void {
  const entry = this.#entries.get(fleetId);
  if (!entry) return;
  const fleet = mergeFacts(entry.snapshot.fleet, facts);
  if (fleet !== entry.snapshot.fleet) patchSnapshot(entry, { fleet });
}

private releaseSubscriber(entry: LiveEntry, fleetId: string, listener: Listener): void {
  entry.listeners.delete(listener);
  entry.refCount -= 1;
  if (entry.refCount > 0) return;
  entry.idleTimer = setTimeout(() => this.teardown(entry, fleetId), IDLE_RELEASE_MS);
}

getSnapshot(fleetId: string): FleetStreamSnapshot {
  return this.#entries.get(fleetId)?.snapshot ?? EMPTY_SNAPSHOT;
}

appendOptimistic(
  fleetId: string,
  text: string,
  actor: string,
  sentAs?: string,
): string {
  const entry = this.#entries.get(fleetId);
  if (!entry) return "";
  tempCounter += 1;
  const tempId = `optim-${tempCounter}`;
  setEvents(entry, (prev) => [...prev, optimisticRow(tempId, text, actor, sentAs)]);
  return tempId;
}

reconcileOptimistic(
  fleetId: string,
  tempId: string,
  realEventId: string,
  replayed: boolean,
): boolean {
  const entry = this.#entries.get(fleetId);
  if (!entry) return false;
  const { events, alreadyComplete, loaded } = reconcileRows(entry.snapshot.events, tempId, realEventId);
  setEvents(entry, () => events);
  // A replay's event may have run before this page held it, with no frame left
  // to settle its row, so it is read now. One the page held settles from frames.
  watchReply(entry, fleetId, realEventId, entry.apply, entry.isCurrent, replayed && !loaded);
  landFrames(entry, fleetId, entry.held.release(entry.snapshot.events));
  return alreadyComplete;
}

// A refused send leaves the thread here. Its text goes back to the composer,
// so the row would only duplicate what the operator is about to resend.
discardOptimistic(fleetId: string, tempId: string): void {
  const entry = this.#entries.get(fleetId);
  if (!entry) return;
  setEvents(entry, (prev) => prev.filter((event) => event.id !== tempId));
  landFrames(entry, fleetId, entry.held.release(entry.snapshot.events));
}

/** Closes streams and clears every cached reply on identity change or logout. */
close(): void {
  for (const [id, e] of this.#entries.entries()) this.teardown(e, id);
}
}
