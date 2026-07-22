import { streamFleetEventsUrl, type EventRow, type LiveFrame } from "@/lib/api/events";
import { outcomeForStatus } from "@/lib/events/event-summary";
import { runBackfill, warnBackfillFailure } from "./fleet-stream-backfill";
import {
  FAST_RECONNECT_ATTEMPTS,
  OFFLINE_RETRY_MS,
  attachRecoveryListeners,
  cancelPendingReconnect,
  fastBackoffMs,
} from "./fleet-stream-reconnect";
import {
  applyLiveFrame,
  mergeBackfill,
  type FleetEvent,
} from "./fleet-stream-frames";
import { advanceInstallStep, installStepFromKind } from "./install-steps";

export {
  type FleetEvent,
  type FleetEventStatus,
} from "./fleet-stream-frames";
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

const STATUS_OPTIMISTIC = "optimistic";
const STATUS_FAILED = "failed";
const STATUS_RECEIVED = "received";

const REGISTRY = new Map<string, Entry>();

const IDLE_RELEASE_MS = 30_000;

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
): void {
  entry.snapshot = { ...entry.snapshot, events: next(entry.snapshot.events) };
  notify(entry);
}

function startEventSource(entry: Entry, fleetId: string): void {
  const url = streamFleetEventsUrl(entry.workspaceId, fleetId);
  const es = new EventSource(url);
  entry.eventSource = es;
  es.onopen = () => {
    const needsBackfill = entry.hasConnectedOnce || entry.hadConnectionError;
    entry.hasConnectedOnce = true;
    entry.hadConnectionError = false;
    // Deliberately NOT resetting reconnectAttempts here. A TCP/SSE open is not
    // proof of a working stream — an unhealthy upstream can accept and close
    // immediately. Attempts reset only once a real frame arrives (onFrame), so
    // an accept-then-close upstream escalates to the slow cadence instead of
    // hammering at the base delay forever.
    patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.LIVE });
    if (needsBackfill) void backfillMissedFrames(entry, fleetId);
  };
  es.onmessage = (e) => {
    // A delivered frame is proof the stream works: return to fast backoff.
    entry.reconnectAttempts = 0;
    onFrame(entry, e);
  };
  es.onerror = () => onEventSourceError(entry, fleetId);
}

// Fire the reconnect gap-recovery walk. The watermark advances only on a
// completed (or explicitly-truncated) walk; a failure leaves it at the anchor
// so the next reconnect retries the same window. Merges are id-deduped, so the
// retry is idempotent.
async function backfillMissedFrames(entry: Entry, fleetId: string): Promise<void> {
  if (entry.backfillInFlight) return;
  entry.backfillInFlight = true;
  try {
    const outcome = await runBackfill({
      workspaceId: entry.workspaceId,
      fleetId,
      anchorMs: entry.serverSinceMs,
      stillCurrent: () => REGISTRY.get(fleetId) === entry,
      onPage: (rows) => setEvents(entry, (prev) => mergeBackfill(prev, rows)),
    });
    if (outcome.ok) entry.serverSinceMs = outcome.watermark;
  } catch (err) {
    warnBackfillFailure(err);
  } finally {
    entry.backfillInFlight = false;
  }
}

function onFrame(entry: Entry, e: MessageEvent): void {
  let parsed: unknown;
  try {
    parsed = JSON.parse(e.data);
  } catch {
    return;
  }
  // SSE payloads are untrusted — validate shape before trusting the cast.
  if (!parsed || typeof parsed !== "object" || typeof (parsed as { kind?: unknown }).kind !== "string") {
    return;
  }
  const frame = parsed as LiveFrame;
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
  setEvents(entry, (prev) => applyLiveFrame(prev, frame));
}

// A lost connection is a transient state, never a terminal one. The fast
// attempts run first; after them the connection is reported as not live but
// the client keeps retrying on an unhurried cadence, so an outage that ends
// while the operator is reading recovers without them doing anything.
function onEventSourceError(entry: Entry, fleetId: string): void {
  entry.eventSource?.close();
  entry.eventSource = null;
  entry.hadConnectionError = true;
  if (entry.reconnectTimer) return;
  entry.reconnectAttempts += 1;
  const exhausted = entry.reconnectAttempts > FAST_RECONNECT_ATTEMPTS;
  patchSnapshot(entry, {
    connectionStatus: exhausted
      ? CONNECTION_STATUS.OFFLINE
      : CONNECTION_STATUS.RECONNECTING,
  });
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
  entry.eventSource?.close();
  entry.eventSource = null;
  entry.reconnectAttempts = 0;
  patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.CONNECTING });
  startEventSource(entry, fleetId);
}

function teardown(entry: Entry, fleetId: string): void {
  cancelPendingReconnect(entry);
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  entry.detachRecovery?.();
  entry.detachRecovery = null;
  entry.eventSource?.close();
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
      hasConnection: () => tracked.eventSource !== null,
      recover: () => {
        cancelPendingReconnect(tracked);
        tracked.reconnectAttempts = 0;
        patchSnapshot(tracked, { connectionStatus: CONNECTION_STATUS.CONNECTING });
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
  setEvents(entry, (prev) => [
    ...prev,
    {
      id: tempId,
      role: "user",
      actor,
      text,
      // The operator's own message is the trigger; the fleet has not replied
      // yet, so the reply is empty and the outcome floor is set for shape.
      reply: "",
      outcome: outcomeForStatus(STATUS_RECEIVED),
      createdAt: new Date(),
      status: STATUS_OPTIMISTIC,
    },
  ]);
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
    const serverEvent = prev.find((event) => event.id === realEventId);
    if (serverEvent) {
      alreadyComplete = serverEvent.status !== STATUS_RECEIVED;
      // A live EVENT_RECEIVED frame carries no message body, so a server row
      // that landed before this reconcile holds an empty trigger. The
      // optimistic row is the only holder of the operator's text — graft it
      // onto the server row before dropping the temp row, or the message
      // blanks out of the thread until a reload.
      const temp = prev.find((event) => event.id === tempId);
      const grafted =
        temp !== undefined && serverEvent.text.length === 0
          ? prev.map((event) =>
              event === serverEvent ? { ...event, text: temp.text } : event,
            )
          : prev;
      return grafted.filter((event) => event.id !== tempId);
    }
    return prev.map((event) =>
      event.id === tempId
        ? { ...event, id: realEventId, status: STATUS_RECEIVED }
        : event,
    );
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
    prev.map((ev) => (ev.id === tempId ? { ...ev, status: STATUS_FAILED } : ev)),
  );
}

// Test surface — vitest must reset between tests; nothing in production
// should call this.
export function __resetRegistryForTests(): void {
  for (const [id, e] of REGISTRY.entries()) teardown(e, id);
  tempCounter = 0;
}
