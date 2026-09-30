import { type EventRow } from "@/lib/api/events";
import { FRAME_KIND, streamFleetEventsUrl } from "@/lib/api/events-types";
import type { FleetFacts } from "@/lib/events/run-summary";
import { backfillEntry } from "./fleet-stream-backfill";
import { factsOf, mergeFacts } from "./fleet-stream-facts";
import { optimisticRow, reconcileRows } from "./fleet-stream-optimistic";
import { patchSnapshot, patchSpokenFacts, setEvents } from "./fleet-stream-snapshot";
import {
  FAST_RECONNECT_ATTEMPTS,
  OFFLINE_RETRY_MS,
  attachRecoveryListeners,
  cancelPendingReconnect,
  fastBackoffMs,
} from "./fleet-stream-reconnect";
import { applyLiveFrame, mergeBackfill, parseLiveFrame } from "./fleet-stream-frames";
import {
  dispatchReplyFrame,
  disposeReplyStreams,
  markReplyGap,
  readStalledReplies,
  settleRepliesFromBackfill,
  watchReply,
  watchRunningRows,
  type ApplyEvents,
} from "./fleet-stream-reply-registry";
import { HEARTBEAT_EVENT } from "./stream-recovery-window";
import { AGENTSFLEET_EVENT_STATUS } from "./fleet-stream-row";
import { advanceInstallStep, installStepFromKind } from "./install-steps";
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

// Module-level subscription registry. One Entry per fleetId; multiple
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

// An entry the registry holds, with the write every reply helper makes and its
// check that this entry still owns the fleet. Both are made once, when the
// entry is adopted, so a frame allocates neither.
type LiveEntry = Entry & { apply: ApplyEvents; isCurrent: () => boolean };

const REGISTRY = new Map<string, LiveEntry>();

const IDLE_RELEASE_MS = 30_000;
const RUNNER_ACTIVITY_KINDS: ReadonlySet<string> = new Set([
  FRAME_KIND.CHUNK, FRAME_KIND.TOOL_CALL_STARTED,
  FRAME_KIND.TOOL_CALL_PROGRESS, FRAME_KIND.TOOL_CALL_COMPLETED,
]);
const TERMINAL_STATUSES: ReadonlySet<string> = new Set([
  AGENTSFLEET_EVENT_STATUS.PROCESSED,
  AGENTSFLEET_EVENT_STATUS.AGENT_ERROR,
  AGENTSFLEET_EVENT_STATUS.GATE_BLOCKED,
]);

// Module-level, not per-entry: a pending send's optimistic row deliberately
// outlives the stream entry, which is torn down after the idle window and
// recreated with fresh state. A per-entry counter restarting at 1 would let a
// stale tempId collide with a new row's id — and a refusal's discard would
// then remove the operator's newest pending message.
let tempCounter = 0;

function adopt(workspaceId: string, fleetId: string, initial: EventRow[]): LiveEntry {
  const entry: LiveEntry = {
    ...createEntry(workspaceId, initial),
    apply: (next, facts) => setEvents(entry, next, facts),
    isCurrent: () => REGISTRY.get(fleetId) === entry,
  };
  watchRunningRows(entry, initial);
  REGISTRY.set(fleetId, entry);
  return entry;
}

function startEventSource(entry: LiveEntry, fleetId: string): void {
  const url = streamFleetEventsUrl(entry.workspaceId, fleetId);
  const es = new EventSource(url);
  entry.eventSource = es;
  const onTimeout = () => { if (entry.eventSource === es) onEventSourceError(entry, fleetId); };
  const received = () => {
    entry.recoveryWindow.received(onTimeout);
    if (entry.recoveryWindow.isStable()) entry.reconnectAttempts = 0;
    if (entry.snapshot.connectionStatus !== CONNECTION_STATUS.LIVE) {
      patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.LIVE });
    }
  };
  // After the frame is recorded, so the frame that ends a long silence counts
  // as heard and its own event is not read back.
  const sweep = () => readStalledReplies(entry, fleetId, entry.apply, entry.isCurrent);
  es.onopen = () => {
    if (entry.eventSource !== es) return;
    entry.recoveryWindow.opened(onTimeout);
    const needsBackfill = entry.hasConnectedOnce || entry.hadConnectionError;
    entry.hasConnectedOnce = true;
    entry.hadConnectionError = false;
    // An open alone does not reset failure history: accept-close loops back off.
    if (needsBackfill) recoverGap(entry, fleetId);
  };
  const handleFrame = (e: MessageEvent) => {
    if (entry.eventSource !== es) return;
    const frame = parseLiveFrame(e.data);
    if (!frame) return;
    received();
    onFrame(entry, fleetId, frame);
    sweep();
  };
  // Named frames dispatch only to their matching listener, never onmessage.
  // Keep both paths: the daemon uses message for its no-kind fallback.
  for (const name of Object.values(FRAME_KIND)) {
    es.addEventListener(name, handleFrame as (e: Event) => void);
  }
  es.onmessage = handleFrame;
  es.addEventListener(HEARTBEAT_EVENT, () => {
    if (entry.eventSource !== es) return;
    received();
    sweep();
  });
  es.onerror = onTimeout;
  entry.recoveryWindow.connecting(onTimeout);
}

// Reads back what the stream missed. A burst of gap signals during a walk
// costs one more walk after it, not one each.
function recoverGap(entry: LiveEntry, fleetId: string): void {
  void backfillEntry(entry, fleetId, {
    stillCurrent: entry.isCurrent,
    onPage: (rows) => {
      setEvents(entry, (prev) => mergeBackfill(prev, rows));
      watchRunningRows(entry, rows);
      settleRepliesFromBackfill(entry, fleetId, rows, entry.apply, entry.isCurrent);
    },
  });
}

function onFrame(entry: LiveEntry, fleetId: string, frame: NonNullable<ReturnType<typeof parseLiveFrame>>): void {
  // The daemon lost frames for this stream — dropped behind a slow reader, or
  // a subscription lost and re-established — so read them back as a reconnect does.
  if (frame.kind === FRAME_KIND.CATCHING_UP) {
    recoverGap(entry, fleetId);
    return;
  }
  // Install frames advance the install step, never the message list. Forking
  // here (rather than inside applyLiveFrame) keeps the chat reducer pure and the
  // two concerns — a long-lived chat timeline vs. a one-shot install beat —
  // independent while sharing the single EventSource the spec mandates.
  const installStep = installStepFromKind(frame.kind);
  if (installStep !== null) {
    patchSnapshot(entry, {
      installStep: advanceInstallStep(entry.snapshot.installStep, installStep),
    });
    return;
  }
  // Best-effort activity can arrive after the report's durable close. Keep
  // every late runner frame from mutating the settled answer or tool history.
  if ("event_id" in frame && RUNNER_ACTIVITY_KINDS.has(frame.kind)
    && entry.snapshot.events.some((event) => event.id === frame.event_id && TERMINAL_STATUSES.has(event.status))) return;
  if (entry.held.take(frame, entry.snapshot.events)) return;
  if (dispatchReplyFrame(entry, fleetId, frame, entry.apply, entry.isCurrent)) return;
  // A completion carries the fleet's status and pending count beside its row;
  // a gate frame carries the count alone and touches no row.
  const facts = factsOf(frame);
  if (frame.kind === FRAME_KIND.GATE_OPENED || frame.kind === FRAME_KIND.GATE_RESOLVED) {
    patchSpokenFacts(entry, facts);
    return;
  }
  setEvents(entry, (prev) => applyLiveFrame(prev, frame), facts);
}

// A lost connection is a transient state, never a terminal one. The fast
// attempts run first; after them the connection is reported as not live but
// the client keeps retrying on an unhurried cadence, so an outage that ends
// while the operator is reading recovers without them doing anything.
function onEventSourceError(entry: LiveEntry, fleetId: string): void {
  markReplyGap(entry);
  entry.eventSource?.close();
  entry.eventSource = null;
  entry.hadConnectionError = true;
  if (entry.recoveryWindow.isStable()) entry.reconnectAttempts = 0;
  entry.reconnectAttempts += 1;
  const exhausted = entry.reconnectAttempts > FAST_RECONNECT_ATTEMPTS;
  entry.recoveryWindow.reportLoss(() => patchSnapshot(entry, {
    connectionStatus: entry.reconnectAttempts > FAST_RECONNECT_ATTEMPTS
      ? CONNECTION_STATUS.OFFLINE
      : CONNECTION_STATUS.RECONNECTING,
  }));
  entry.reconnectTimer = setTimeout(
    () => {
      entry.reconnectTimer = null;
      startEventSource(entry, fleetId);
    },
    exhausted ? OFFLINE_RETRY_MS : fastBackoffMs(entry.reconnectAttempts),
  );
}

export function retryConnection(fleetId: string): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return;
  cancelPendingReconnect(entry);
  entry.recoveryWindow.dispose();
  entry.eventSource?.close();
  entry.eventSource = null;
  entry.reconnectAttempts = 0;
  patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.CONNECTING });
  startEventSource(entry, fleetId);
}

function teardown(entry: Entry, fleetId: string): void {
  disposeReplyStreams(entry);
  cancelPendingReconnect(entry);
  entry.recoveryWindow.dispose();
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  entry.detachRecovery?.();
  entry.detachRecovery = null;
  entry.eventSource?.close();
  entry.eventSource = null;
  REGISTRY.delete(fleetId);
}

export function subscribe(
  workspaceId: string,
  fleetId: string,
  initial: EventRow[],
  listener: Listener,
): () => void {
  let entry = REGISTRY.get(fleetId);
  if (!entry) {
    entry = adopt(workspaceId, fleetId, initial);
    const tracked = entry;
    entry.detachRecovery = attachRecoveryListeners({
      hasConnection: () => tracked.eventSource !== null && !tracked.recoveryWindow.isStale(),
      recover: () => {
        if (tracked.eventSource) onEventSourceError(tracked, fleetId);
        cancelPendingReconnect(tracked);
        tracked.reconnectAttempts = 0;
        if (tracked.snapshot.connectionStatus !== CONNECTION_STATUS.LIVE) {
          patchSnapshot(tracked, { connectionStatus: CONNECTION_STATUS.CONNECTING });
        }
        startEventSource(tracked, fleetId);
      },
    });
    startEventSource(entry, fleetId);
  }
  if (entry.idleTimer) {
    clearTimeout(entry.idleTimer);
    entry.idleTimer = null;
  }
  entry.refCount += 1;
  entry.listeners.add(listener);
  return () => releaseSubscriber(fleetId, listener);
}

export function reconcileServerRows(fleetId: string, rows: EventRow[]): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry || rows.length === 0) return;
  setEvents(entry, (prev) => mergeBackfill(prev, rows));
  watchRunningRows(entry, rows);
}

// A server render's word on the fleet, as the page just read it. It overwrites
// what the tail last said — a kill from the header reaches the strip this way,
// since no frame announces a PATCH — and the next frame overwrites it back.
// Whether a render is older than a frame that landed while it was in flight
// is the caller's to decide, from `factsSeq`; `factsSeq` never moves here.
export function reconcileServerFacts(fleetId: string, facts: FleetFacts): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return;
  const fleet = mergeFacts(entry.snapshot.fleet, facts);
  if (fleet !== entry.snapshot.fleet) patchSnapshot(entry, { fleet });
}

function releaseSubscriber(fleetId: string, listener: Listener): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return;
  entry.listeners.delete(listener);
  entry.refCount -= 1;
  if (entry.refCount > 0) return;
  entry.idleTimer = setTimeout(() => teardown(entry, fleetId), IDLE_RELEASE_MS);
}

export function getSnapshot(fleetId: string): FleetStreamSnapshot {
  return REGISTRY.get(fleetId)?.snapshot ?? EMPTY_SNAPSHOT;
}

export function appendOptimistic(
  fleetId: string,
  text: string,
  actor: string,
): string {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return "";
  tempCounter += 1;
  const tempId = `optim-${tempCounter}`;
  setEvents(entry, (prev) => [...prev, optimisticRow(tempId, text, actor)]);
  return tempId;
}

export function reconcileOptimistic(
  fleetId: string,
  tempId: string,
  realEventId: string,
  replayed: boolean,
): boolean {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return false;
  const { events, alreadyComplete, loaded } = reconcileRows(entry.snapshot.events, tempId, realEventId);
  setEvents(entry, () => events);
  // A replay's event may have run before this page held it, with no frame left
  // to settle its row, so it is read now. One the page held settles from frames.
  watchReply(entry, fleetId, realEventId, entry.apply, entry.isCurrent, replayed && !loaded);
  for (const frame of entry.held.release(entry.snapshot.events)) onFrame(entry, fleetId, frame);
  return alreadyComplete;
}

// A refused send leaves the thread here. Its text goes back to the composer,
// so the row would only duplicate what the operator is about to resend.
export function discardOptimistic(fleetId: string, tempId: string): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return;
  setEvents(entry, (prev) => prev.filter((event) => event.id !== tempId));
  for (const frame of entry.held.release(entry.snapshot.events)) onFrame(entry, fleetId, frame);
}

// Test surface — vitest must reset between tests; nothing in production
// should call this.
export function __resetRegistryForTests(): void {
  for (const [id, e] of REGISTRY.entries()) teardown(e, id);
  tempCounter = 0;
}
