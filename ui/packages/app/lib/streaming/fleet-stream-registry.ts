import { type EventRow } from "@/lib/api/events";
import { FRAME_KIND, streamFleetEventsUrl } from "@/lib/api/events-types";
import { latestFigures, sameFigures, type FleetFacts } from "@/lib/events/run-summary";
import { backfillEntry } from "./fleet-stream-backfill";
import { factsOf, mergeFacts } from "./fleet-stream-facts";
import { optimisticRow, reconcileRows } from "./fleet-stream-optimistic";
import {
  FAST_RECONNECT_ATTEMPTS,
  OFFLINE_RETRY_MS,
  attachRecoveryListeners,
  cancelPendingReconnect,
  fastBackoffMs,
} from "./fleet-stream-reconnect";
import { applyLiveFrame, mergeBackfill, parseLiveFrame } from "./fleet-stream-frames";
import { dispatchReplyFrame, disposeReplyStreams, markReplyGap, settleRepliesFromBackfill } from "./fleet-stream-reply-registry";
import { HEARTBEAT_EVENT } from "./stream-recovery-window";
import { AGENTSFLEET_EVENT_STATUS, type FleetEvent } from "./fleet-stream-row";
import { advanceInstallStep, installStepFromKind } from "./install-steps";
import { capEvents } from "./fleet-stream-cap";
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

const REGISTRY = new Map<string, Entry>();

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

// Module-level, not per-entry: a FailedDelivery (and the tempId it stores)
// deliberately outlives the stream entry, which is torn down after the idle
// window and recreated with fresh state. A per-entry counter restarting at 1
// would let a stale stored tempId collide with a new row's id — and retry's
// discard would then remove the operator's newest pending message.
let tempCounter = 0;

function notify(entry: Entry): void {
  for (const l of entry.listeners) l();
}

function patchSnapshot(entry: Entry, patch: Partial<FleetStreamSnapshot>): void {
  entry.snapshot = { ...entry.snapshot, ...patch };
  notify(entry);
}

function setEvents(
  entry: Entry,
  next: (prev: FleetEvent[]) => FleetEvent[],
  spoken: Partial<FleetFacts> = {},
): void {
  // The one choke point every mutation flows through, so the cap lives here
  // once rather than at all eight call sites — and so does the strip's
  // `latest`, recomputed from the rows and kept by identity when unchanged.
  // A completion's fleet facts fold into the same write, so the frame costs
  // its subscribers one notification, not two.
  const events = capEvents(next(entry.snapshot.events));
  const latest = latestFigures(events);
  entry.snapshot = {
    ...entry.snapshot,
    ...spokenFacts(entry, spoken),
    events,
    latest: sameFigures(latest, entry.snapshot.latest) ? entry.snapshot.latest : latest,
  };
  notify(entry);
}

// What a FRAME said about the fleet itself, as a snapshot patch: the merged
// facts and the advanced sequence, or nothing when the frame restated what
// the snapshot already held.
function spokenFacts(entry: Entry, patch: Partial<FleetFacts>): Partial<FleetStreamSnapshot> {
  const fleet = mergeFacts(entry.snapshot.fleet, patch);
  if (fleet === entry.snapshot.fleet) return {};
  return { fleet, factsSeq: entry.snapshot.factsSeq + 1 };
}

// A gate frame moves the count and no row. Nothing is notified when nothing
// changed, so a frame restating the count does not wake anyone.
function patchSpokenFacts(entry: Entry, patch: Partial<FleetFacts>): void {
  const spoken = spokenFacts(entry, patch);
  if (spoken.fleet !== undefined) patchSnapshot(entry, spoken);
}

function startEventSource(entry: Entry, fleetId: string): void {
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
  es.onopen = () => {
    if (entry.eventSource !== es) return;
    entry.recoveryWindow.opened(onTimeout);
    const needsBackfill = entry.hasConnectedOnce || entry.hadConnectionError;
    entry.hasConnectedOnce = true;
    entry.hadConnectionError = false;
    // An open alone does not reset failure history: accept-close loops back off.
    if (needsBackfill) {
      void backfillEntry(entry, fleetId, {
        stillCurrent: () => REGISTRY.get(fleetId) === entry,
        onPage: (rows) => {
          setEvents(entry, (prev) => mergeBackfill(prev, rows));
          settleRepliesFromBackfill(entry, fleetId, rows,
            (next, facts) => setEvents(entry, next, facts),
            () => REGISTRY.get(fleetId) === entry);
        },
      });
    }
  };
  const handleFrame = (e: MessageEvent) => {
    if (entry.eventSource !== es) return;
    const frame = parseLiveFrame(e.data);
    if (!frame) return;
    received();
    onFrame(entry, fleetId, frame);
  };
  // Named frames dispatch only to their matching listener, never onmessage.
  // Keep both paths: the daemon uses message for its no-kind fallback.
  for (const name of Object.values(FRAME_KIND)) {
    es.addEventListener(name, handleFrame as (e: Event) => void);
  }
  es.onmessage = handleFrame;
  es.addEventListener(HEARTBEAT_EVENT, () => { if (entry.eventSource === es) received(); });
  es.onerror = onTimeout;
  entry.recoveryWindow.connecting(onTimeout);
}

function onFrame(entry: Entry, fleetId: string, frame: NonNullable<ReturnType<typeof parseLiveFrame>>): void {
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
  if (dispatchReplyFrame(entry, fleetId, frame,
    (next, facts) => setEvents(entry, next, facts),
    () => REGISTRY.get(fleetId) === entry)) return;
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
function onEventSourceError(entry: Entry, fleetId: string): void {
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
    entry = createEntry(workspaceId, initial);
    REGISTRY.set(fleetId, entry);
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
): boolean {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return false;
  let alreadyComplete = false;
  setEvents(entry, (prev) => {
    const reconciled = reconcileRows(prev, tempId, realEventId);
    alreadyComplete = reconciled.alreadyComplete;
    return reconciled.events;
  });
  return alreadyComplete;
}

// A failed optimistic row being retried leaves the thread here: the retry
// re-submits the same text as a fresh optimistic row, so keeping the stale
// failed copy would stack a duplicate of the same operator message on every
// attempt.
export function discardOptimistic(fleetId: string, tempId: string): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return;
  setEvents(entry, (prev) => prev.filter((event) => event.id !== tempId));
}

// A steer that failed server-side (the Server Action returned ok:false
// after its retries). The optimistic row keeps its tempId but flips to
// `failed` so the renderer can paint a destructive badge instead of the
// `queued` one — the user sees the send did not land.
export function markOptimisticFailed(fleetId: string, tempId: string): void {
  const entry = REGISTRY.get(fleetId);
  if (!entry) return;
  setEvents(entry, (prev) =>
    prev.map((ev) =>
      ev.id === tempId ? { ...ev, status: AGENTSFLEET_EVENT_STATUS.FAILED } : ev,
    ),
  );
}

// Test surface — vitest must reset between tests; nothing in production
// should call this.
export function __resetRegistryForTests(): void {
  for (const [id, e] of REGISTRY.entries()) teardown(e, id);
  tempCounter = 0;
}
