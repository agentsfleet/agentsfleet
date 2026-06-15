import { streamAgentEventsUrl, type EventRow, type LiveFrame } from "@/lib/api/events";
import {
  applyLiveFrame,
  mergeBackfill,
  type AgentEvent,
} from "./agent-stream-frames";

export {
  type AgentEvent,
  type AgentEventStatus,
} from "./agent-stream-frames";

// Module-level subscription registry. One Entry per agentId; multiple
// React hook instances share it via refcounted subscribe/release. The
// EventSource survives a /dashboard ↔ /agents/[id] round-trip up to
// IDLE_RELEASE_MS after the last consumer detaches — anything longer
// and we tear down so a never-revisited tab doesn't leak a connection.
//
// The initial event list is seeded from server-rendered data passed by
// the caller (no client-side backfill GET, no bearer token in the
// browser); live updates ride the cookie-authed SSE route handler.

export const CONNECTION_STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
} as const;
export type ConnectionStatus =
  (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];

const STATUS_OPTIMISTIC = "optimistic";
const STATUS_FAILED = "failed";
const STATUS_RECEIVED = "received";

export type AgentStreamSnapshot = {
  events: AgentEvent[];
  connectionStatus: ConnectionStatus;
};

type Listener = () => void;

type Entry = {
  workspaceId: string;
  snapshot: AgentStreamSnapshot;
  listeners: Set<Listener>;
  refCount: number;
  eventSource: EventSource | null;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectAttempts: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  tempCounter: number;
};

const REGISTRY = new Map<string, Entry>();

const IDLE_RELEASE_MS = 30_000;
const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;

const EMPTY_SNAPSHOT: AgentStreamSnapshot = Object.freeze({
  events: [],
  connectionStatus: CONNECTION_STATUS.CONNECTING,
}) as AgentStreamSnapshot;

function notify(entry: Entry): void {
  for (const l of entry.listeners) l();
}

function patchSnapshot(entry: Entry, patch: Partial<AgentStreamSnapshot>): void {
  entry.snapshot = { ...entry.snapshot, ...patch };
  notify(entry);
}

function setEvents(
  entry: Entry,
  next: (prev: AgentEvent[]) => AgentEvent[],
): void {
  entry.snapshot = { ...entry.snapshot, events: next(entry.snapshot.events) };
  notify(entry);
}

function startEventSource(entry: Entry, agentId: string): void {
  if (entry.eventSource) return;
  const url = streamAgentEventsUrl(entry.workspaceId, agentId);
  const es = new EventSource(url);
  entry.eventSource = es;
  es.onopen = () => {
    entry.reconnectAttempts = 0;
    patchSnapshot(entry, { connectionStatus: CONNECTION_STATUS.LIVE });
  };
  es.onmessage = (e) => onFrame(entry, e);
  es.onerror = () => onEventSourceError(entry, agentId);
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
  setEvents(entry, (prev) => applyLiveFrame(prev, frame));
}

function onEventSourceError(entry: Entry, agentId: string): void {
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
    startEventSource(entry, agentId);
  }, delayMs);
}

function teardown(entry: Entry, agentId: string): void {
  if (entry.reconnectTimer) clearTimeout(entry.reconnectTimer);
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  entry.eventSource?.close();
  REGISTRY.delete(agentId);
}

function createEntry(workspaceId: string, initial: EventRow[]): Entry {
  return {
    workspaceId,
    snapshot: {
      events: mergeBackfill([], initial),
      connectionStatus: CONNECTION_STATUS.CONNECTING,
    },
    listeners: new Set(),
    refCount: 0,
    eventSource: null,
    reconnectTimer: null,
    reconnectAttempts: 0,
    idleTimer: null,
    tempCounter: 0,
  };
}

export function subscribe(
  workspaceId: string,
  agentId: string,
  initial: EventRow[],
  listener: Listener,
): () => void {
  let entry = REGISTRY.get(agentId);
  if (!entry) {
    entry = createEntry(workspaceId, initial);
    REGISTRY.set(agentId, entry);
    startEventSource(entry, agentId);
  }
  if (entry.idleTimer) {
    clearTimeout(entry.idleTimer);
    entry.idleTimer = null;
  }
  entry.refCount += 1;
  entry.listeners.add(listener);
  return () => releaseSubscriber(agentId, listener);
}

function releaseSubscriber(agentId: string, listener: Listener): void {
  const entry = REGISTRY.get(agentId);
  if (!entry) return;
  entry.listeners.delete(listener);
  entry.refCount -= 1;
  if (entry.refCount > 0) return;
  entry.idleTimer = setTimeout(() => teardown(entry, agentId), IDLE_RELEASE_MS);
}

export function getSnapshot(agentId: string): AgentStreamSnapshot {
  return REGISTRY.get(agentId)?.snapshot ?? EMPTY_SNAPSHOT;
}

export function appendOptimistic(
  agentId: string,
  text: string,
  actor: string,
): string {
  const entry = REGISTRY.get(agentId);
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
  agentId: string,
  tempId: string,
  realEventId: string,
): void {
  const entry = REGISTRY.get(agentId);
  if (!entry) return;
  setEvents(entry, (prev) =>
    prev.map((ev) =>
      ev.id === tempId ? { ...ev, id: realEventId, status: STATUS_RECEIVED } : ev,
    ),
  );
}

// A steer that failed server-side (the Server Action returned ok:false
// after its retries). The optimistic row keeps its tempId but flips to
// `failed` so the renderer can paint a destructive badge instead of the
// `queued` one — the user sees the send did not land.
export function markOptimisticFailed(agentId: string, tempId: string): void {
  const entry = REGISTRY.get(agentId);
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
