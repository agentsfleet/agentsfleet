import { FRAME_KIND, streamFleetEventsUrl, type EventRow, type LiveFrame } from "@/lib/api/events";
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
import { applyLiveFrame, mergeBackfill } from "./fleet-stream-frames";
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
    if (needsBackfill) {
      void backfillEntry(entry, fleetId, {
        stillCurrent: () => REGISTRY.get(fleetId) === entry,
        onPage: (rows) => setEvents(entry, (prev) => mergeBackfill(prev, rows)),
      });
    }
  };
  const handleFrame = (e: MessageEvent) => {
    // A delivered frame is proof the stream works: return to fast backoff.
    entry.reconnectAttempts = 0;
    onFrame(entry, e);
  };
  // The daemon names every frame with its payload kind (`event: chunk`,
  // `event: event_complete` — sse_frame.writeHead), and a NAMED Server-Sent
  // Events frame dispatches ONLY to its addEventListener — never to
  // `onmessage`. An onmessage-only client shows a green Live badge (onopen
  // fires, heartbeats flow) while silently dropping every frame; replies then
  // appear only on the next server render. Same wiring as workspace-stream.ts.
  for (const name of Object.values(FRAME_KIND)) {
    es.addEventListener(name, handleFrame as (e: Event) => void);
  }
  // The daemon's fallback for a payload with no leading kind is
  // `event: message`, which is what onmessage receives.
  es.onmessage = handleFrame;
  es.onerror = () => onEventSourceError(entry, fleetId);
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
