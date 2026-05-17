import {
  listZombieEvents,
  streamZombieEventsUrl,
  type LiveFrame,
} from "@/lib/api/events";
import { RETRY_DEFAULTS, type RetryReason } from "@/lib/api/retry";
import {
  applyLiveFrame,
  mergeBackfill,
  type ZombieEvent,
} from "./zombie-stream-frames";

export {
  type ZombieEvent,
  type ZombieEventStatus,
} from "./zombie-stream-frames";

// Module-level subscription registry. One Entry per zombieId; multiple
// React hook instances share it via refcounted subscribe/release. The
// EventSource survives a /dashboard ↔ /zombies/[id] round-trip up to
// IDLE_RELEASE_MS after the last consumer detaches — anything longer
// and we tear down so a never-revisited tab doesn't leak a connection.

export const CONNECTION_STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
} as const;
export type ConnectionStatus =
  (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];

const STATUS_OPTIMISTIC = "optimistic";

export type RetryState = {
  phase: "backfill";
  attempt: number;
  max: number;
  reason: RetryReason;
} | null;

export type ZombieStreamSnapshot = {
  events: ZombieEvent[];
  connectionStatus: ConnectionStatus;
  retryState: RetryState;
};

type Listener = () => void;

type Entry = {
  workspaceId: string;
  // SSE is cookie-authed and still opens when bearer is absent;
  // backfill is bearer-only and is skipped while token is null.
  token: string | null;
  snapshot: ZombieStreamSnapshot;
  listeners: Set<Listener>;
  refCount: number;
  eventSource: EventSource | null;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectAttempts: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  abortBackfill: AbortController | null;
  tempCounter: number;
  backfillStarted: boolean;
};

const REGISTRY = new Map<string, Entry>();

const IDLE_RELEASE_MS = 30_000;
const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;
const BACKFILL_LIMIT = 50;

const EMPTY_SNAPSHOT: ZombieStreamSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,
  retryState: null,
}) as ZombieStreamSnapshot;

function notify(entry: Entry): void {
  for (const l of entry.listeners) l();
}

function patchSnapshot(entry: Entry, patch: Partial<ZombieStreamSnapshot>): void {
  entry.snapshot = { ...entry.snapshot, ...patch };
  notify(entry);
}

function setEvents(
  entry: Entry,
  next: (prev: ZombieEvent[]) => ZombieEvent[],
): void {
  entry.snapshot = { ...entry.snapshot, events: next(entry.snapshot.events) };
  notify(entry);
}

function startBackfill(entry: Entry, zombieId: string): void {
  const token = entry.token;
  if (entry.backfillStarted || !token) return;
  entry.backfillStarted = true;
  const controller = new AbortController();
  entry.abortBackfill = controller;
  void (async () => {
    try {
      const page = await listZombieEvents(
        entry.workspaceId,
        zombieId,
        token,
        { limit: BACKFILL_LIMIT },
        {
          onRetry: ({ attempt, reason }) => {
            if (controller.signal.aborted) return;
            patchSnapshot(entry, {
              retryState: {
                phase: "backfill",
                attempt,
                max: RETRY_DEFAULTS.maxAttempts,
                reason,
              },
            });
          },
          onAttempt: ({ terminal }) => {
            if (!controller.signal.aborted && terminal) {
              patchSnapshot(entry, { retryState: null });
            }
          },
        },
      );
      if (controller.signal.aborted) return;
      setEvents(entry, (prev) => mergeBackfill(prev, page.items));
    } catch {
      // Backfill failures don't surface as a thrown error; the live
      // stream's connection state is the authoritative health signal.
    }
  })();
}

function startEventSource(entry: Entry, zombieId: string): void {
  if (entry.eventSource) return;
  const url = streamZombieEventsUrl(entry.workspaceId, zombieId);
  const es = new EventSource(url);
  entry.eventSource = es;
  es.onopen = () => {
    entry.reconnectAttempts = 0;
    patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.LIVE });
  };
  es.onmessage = (e) => onFrame(entry, e);
  es.onerror = () => onEventSourceError(entry, zombieId);
}

function onFrame(entry: Entry, e: MessageEvent): void {
  let parsed: LiveFrame | null = null;
  try {
    parsed = JSON.parse(e.data) as LiveFrame;
  } catch {
    return;
  }
  if (!parsed || typeof parsed !== "object" || typeof parsed.kind !== "string") {
    return;
  }
  const frame = parsed;
  setEvents(entry, (prev) => applyLiveFrame(prev, frame));
}

function onEventSourceError(entry: Entry, zombieId: string): void {
  entry.eventSource?.close();
  entry.eventSource = null;
  patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.RECONNECTING });
  entry.reconnectAttempts += 1;
  const delayMs = Math.min(
    RECONNECT_BACKOFF_BASE_MS *
      2 ** Math.min(entry.reconnectAttempts, RECONNECT_MAX_BACKOFF_ATTEMPTS),
    RECONNECT_BACKOFF_CAP_MS,
  );
  entry.reconnectTimer = setTimeout(() => {
    entry.reconnectTimer = null;
    startEventSource(entry, zombieId);
  }, delayMs);
}

function teardown(entry: Entry, zombieId: string): void {
  entry.abortBackfill?.abort();
  if (entry.reconnectTimer) clearTimeout(entry.reconnectTimer);
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  entry.eventSource?.close();
  REGISTRY.delete(zombieId);
}

function createEntry(workspaceId: string, token: string | null): Entry {
  return {
    workspaceId,
    token,
    snapshot: {
      events: [],
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      retryState: null,
    },
    listeners: new Set(),
    refCount: 0,
    eventSource: null,
    reconnectTimer: null,
    reconnectAttempts: 0,
    idleTimer: null,
    abortBackfill: null,
    tempCounter: 0,
    backfillStarted: false,
  };
}

export function subscribe(
  workspaceId: string,
  zombieId: string,
  token: string | null,
  listener: Listener,
): () => void {
  let entry = REGISTRY.get(zombieId);
  if (!entry) {
    entry = createEntry(workspaceId, token);
    REGISTRY.set(zombieId, entry);
    startEventSource(entry, zombieId);
    startBackfill(entry, zombieId);
  } else if (!entry.token && token) {
    // First subscriber arrived without a token (SSE-only); a later
    // subscriber brought one — upgrade the entry and kick off the
    // backfill that was previously skipped.
    entry.token = token;
    startBackfill(entry, zombieId);
  }
  if (entry.idleTimer) {
    clearTimeout(entry.idleTimer);
    entry.idleTimer = null;
  }
  entry.refCount += 1;
  entry.listeners.add(listener);
  return () => releaseSubscriber(zombieId, listener);
}

function releaseSubscriber(zombieId: string, listener: Listener): void {
  const entry = REGISTRY.get(zombieId);
  if (!entry) return;
  entry.listeners.delete(listener);
  entry.refCount -= 1;
  if (entry.refCount > 0) return;
  entry.idleTimer = setTimeout(() => teardown(entry, zombieId), IDLE_RELEASE_MS);
}

export function getSnapshot(zombieId: string): ZombieStreamSnapshot {
  return REGISTRY.get(zombieId)?.snapshot ?? EMPTY_SNAPSHOT;
}

export function appendOptimistic(
  zombieId: string,
  text: string,
  actor: string,
): string {
  const entry = REGISTRY.get(zombieId);
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
      createdAt: new Date(),
      status: STATUS_OPTIMISTIC,
    },
  ]);
  return tempId;
}

export function reconcileOptimistic(
  zombieId: string,
  tempId: string,
  realEventId: string,
): void {
  const entry = REGISTRY.get(zombieId);
  if (!entry) return;
  setEvents(entry, (prev) =>
    prev.map((ev) =>
      ev.id === tempId ? { ...ev, id: realEventId, status: "received" } : ev,
    ),
  );
}

// Test surface — vitest must reset between tests; nothing in production
// should call this.
export function __resetRegistryForTests(): void {
  for (const [id, e] of REGISTRY.entries()) teardown(e, id);
}
