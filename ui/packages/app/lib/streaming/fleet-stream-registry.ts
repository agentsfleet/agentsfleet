import { streamFleetEventsUrl, type EventRow, type LiveFrame } from "@/lib/api/events";
import { outcomeForStatus } from "@/lib/events/event-summary";
import { runBackfill, warnBackfillFailure } from "./fleet-stream-backfill";
import {
  applyLiveFrame,
  maxServerCreatedAt,
  mergeBackfill,
  type FleetEvent,
} from "./fleet-stream-frames";
import {
  advanceInstallStep,
  installStepFromKind,
  type InstallStepId,
} from "./install-steps";

export {
  type FleetEvent,
  type FleetEventStatus,
} from "./fleet-stream-frames";

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

export const CONNECTION_STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
  OFFLINE: "offline",
} as const;
export type ConnectionStatus =
  (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];

const STATUS_OPTIMISTIC = "optimistic";
const STATUS_FAILED = "failed";
const STATUS_RECEIVED = "received";

export type FleetStreamSnapshot = {
  events: FleetEvent[];
  connectionStatus: ConnectionStatus;
  // The latest install step advanced by an `install:*` frame, or null when no
  // install frame has arrived (a non-installing fleet, or pre-first-frame). The
  // InstallStates surface reads this to advance its rendered step and to detect
  // the installing→active flip; the chat path ignores it.
  installStep: InstallStepId | null;
};

type Listener = () => void;

type Entry = {
  workspaceId: string;
  snapshot: FleetStreamSnapshot;
  listeners: Set<Listener>;
  refCount: number;
  eventSource: EventSource | null;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectAttempts: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  tempCounter: number;
  // Whether this entry's EventSource has ever reached onopen. Distinguishes
  // the initial (SSR-seeded) connect from a reconnect — only the latter
  // backfills. Lives on the mutable Entry, never a passed primitive.
  hasConnectedOnce: boolean;
  // An outage before the first successful open still creates a history gap.
  // The next open backfills even though `hasConnectedOnce` is still false.
  hadConnectionError: boolean;
  // Newest server-confirmed created_at (epoch ms) — advanced only by the SSR
  // seed and successful backfill pages, never by client-clock-stamped live or
  // optimistic rows, and never by a failed backfill (so a failure cannot seal
  // the gap it left).
  serverSinceMs: number | null;
  backfillInFlight: boolean;
  // Detaches the tab-visible / network-online recovery listeners. Held on the
  // entry so teardown can remove exactly what subscribe attached.
  detachRecovery: (() => void) | null;
};

const REGISTRY = new Map<string, Entry>();

const IDLE_RELEASE_MS = 30_000;
const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;
// How many fast attempts run before the connection is reported as not live.
// Reporting is all that changes at this point — the client keeps trying.
const FAST_RECONNECT_ATTEMPTS = 5;
// The unhurried cadence a not-live connection keeps trying on. An outage that
// outlasts the fast attempts is usually minutes long, not seconds, and the
// operator should not have to press a button to come back from it.
const OFFLINE_RETRY_MS = 30_000;

const EMPTY_SNAPSHOT: FleetStreamSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  installStep: null,
}) as FleetStreamSnapshot;

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
    entry.reconnectAttempts = 0;
    patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.LIVE });
    if (needsBackfill) void backfillMissedFrames(entry, fleetId);
  };
  es.onmessage = (e) => onFrame(entry, e);
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

function fastBackoffMs(attempts: number): number {
  return Math.min(
    RECONNECT_BACKOFF_BASE_MS * 2 ** Math.min(attempts, RECONNECT_MAX_BACKOFF_ATTEMPTS),
    RECONNECT_BACKOFF_CAP_MS,
  );
}

// The two moments a stale connection is both most likely wrong and cheapest to
// re-establish: the tab coming back to the foreground, and the browser
// reporting the network back. Either one skips the remaining wait.
function attachRecoveryListeners(entry: Entry, fleetId: string): () => void {
  const recover = () => {
    // A connection already open or in flight needs nothing; this is what keeps
    // both signals arriving together from opening two streams.
    if (entry.eventSource !== null) return;
    if (document.visibilityState === "hidden") return;
    cancelPendingReconnect(entry);
    entry.reconnectAttempts = 0;
    patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.CONNECTING });
    startEventSource(entry, fleetId);
  };
  document.addEventListener("visibilitychange", recover);
  window.addEventListener("online", recover);
  return () => {
    document.removeEventListener("visibilitychange", recover);
    window.removeEventListener("online", recover);
  };
}

// One place cancels a pending reconnect, so the "is one pending?" question is
// asked identically by the operator's retry, the recovery signals, and teardown.
function cancelPendingReconnect(entry: Entry): void {
  if (entry.reconnectTimer) clearTimeout(entry.reconnectTimer);
  entry.reconnectTimer = null;
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

function createEntry(workspaceId: string, initial: EventRow[]): Entry {
  return {
    workspaceId,
    snapshot: {
      events: mergeBackfill([], initial),
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      installStep: null,
    },
    listeners: new Set(),
    refCount: 0,
    eventSource: null,
    reconnectTimer: null,
    reconnectAttempts: 0,
    idleTimer: null,
    tempCounter: 0,
    hasConnectedOnce: false,
    hadConnectionError: false,
    serverSinceMs: maxServerCreatedAt(null, initial),
    backfillInFlight: false,
    detachRecovery: null,
  };
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
    entry.detachRecovery = attachRecoveryListeners(entry, fleetId);
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
  entry.tempCounter += 1;
  const tempId = `optim-${entry.tempCounter}`;
  setEvents(entry, (prev) => [
    ...prev,
    {
      id: tempId,
      role: "user",
      actor,
      text,
      // A submission always carries its own text, so the outcome floor is
      // never reached — it is set for shape, not for display.
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
      return prev.filter((event) => event.id !== tempId);
    }
    return prev.map((event) =>
      event.id === tempId
        ? { ...event, id: realEventId, status: STATUS_RECEIVED }
        : event,
    );
  });
  return alreadyComplete;
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
}
