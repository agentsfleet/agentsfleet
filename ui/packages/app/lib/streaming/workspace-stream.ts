import {
  FRAME_KIND,
  streamWorkspaceEventsUrl,
  type LiveFrame,
  type WorkspaceControlFrame,
  type WorkspaceFrame,
  type WorkspaceLiveFrame,
} from "@/lib/api/events";

// One EventSource per WORKSPACE, demultiplexed to per-fleet subscribers.
//
// The wall used to open one EventSource per live tile — L×V connections for L
// live fleets and V viewers. This registry opens exactly one connection per
// workspace and routes each `fleet_id`-tagged frame to the tile that subscribed
// for that fleet. A tile subscribes with (workspaceId, fleetId); it receives
// only its own fleet's frames.
//
// Reconnect discipline mirrors fleet-stream-registry.ts: the connection
// survives a route change up to IDLE_RELEASE_MS after its last subscriber
// detaches, reconnects with capped backoff, and — on a reconnect open, never
// the first — backfills the gap through the same-origin workspace events proxy.
//
// Frame safety: a frame whose `data` is not valid JSON, is not an object, has
// no string `kind`, or has no string `fleet_id` is DROPPED. Mis-routing a frame
// to the wrong tile is worse than losing it, and the durable row is recoverable
// through backfill.

export type FleetFrameListener = (frame: WorkspaceLiveFrame) => void;
export type WorkspaceFrameListener = (frame: WorkspaceControlFrame) => void;

export const WORKSPACE_CONNECTION_STATUS = {
  CONNECTING: "connecting",
  LIVE: "live",
  RECONNECTING: "reconnecting",
} as const;
export type WorkspaceConnectionStatus =
  (typeof WORKSPACE_CONNECTION_STATUS)[keyof typeof WORKSPACE_CONNECTION_STATUS];

export type StatusListener = (status: WorkspaceConnectionStatus) => void;

// The gap-recovery hook. The wall injects a workspace-scoped backfill walk here
// (paging the durable events list) so this module stays free of fetch/proxy
// details and is unit-testable without a network. Called on every reconnect
// open, never the first. `sinceMs` is the newest server-confirmed frame time,
// or null before any has arrived.
export type BackfillFn = (workspaceId: string, sinceMs: number | null) => Promise<void>;

const IDLE_RELEASE_MS = 30_000;
const RECONNECT_BACKOFF_BASE_MS = 1_000;
const RECONNECT_BACKOFF_CAP_MS = 15_000;
const RECONNECT_MAX_BACKOFF_ATTEMPTS = 5;

type Entry = {
  workspaceId: string;
  eventSource: EventSource | null;
  status: WorkspaceConnectionStatus;
  // fleetId → the tile listeners watching that fleet. A fleet with no listeners
  // is pruned so the map tracks exactly the live tiles.
  fleetListeners: Map<string, Set<FleetFrameListener>>;
  workspaceListeners: Set<WorkspaceFrameListener>;
  statusListeners: Set<StatusListener>;
  refCount: number;
  reconnectTimer: ReturnType<typeof setTimeout> | null;
  reconnectAttempts: number;
  idleTimer: ReturnType<typeof setTimeout> | null;
  hasConnectedOnce: boolean;
  // Newest server-confirmed frame time (epoch ms) — the backfill anchor.
  serverSinceMs: number | null;
  backfillInFlight: boolean;
  backfill: BackfillFn | null;
};

const REGISTRY = new Map<string, Entry>();

function notifyStatus(entry: Entry): void {
  for (const l of entry.statusListeners) l(entry.status);
}

function setStatus(entry: Entry, status: WorkspaceConnectionStatus): void {
  if (entry.status === status) return;
  entry.status = status;
  notifyStatus(entry);
}

function startEventSource(entry: Entry): void {
  const es = new EventSource(streamWorkspaceEventsUrl(entry.workspaceId));
  entry.eventSource = es;
  es.onopen = () => {
    const isReconnect = entry.hasConnectedOnce;
    entry.hasConnectedOnce = true;
    entry.reconnectAttempts = 0;
    setStatus(entry, WORKSPACE_CONNECTION_STATUS.LIVE);
    if (isReconnect) void backfillGap(entry);
  };
  es.onmessage = (e) => onFrame(entry, e);
  es.onerror = () => onEventSourceError(entry);
}

async function backfillGap(entry: Entry): Promise<void> {
  if (!entry.backfill || entry.backfillInFlight) return;
  entry.backfillInFlight = true;
  try {
    await entry.backfill(entry.workspaceId, entry.serverSinceMs);
  } finally {
    entry.backfillInFlight = false;
  }
}

// Parse + validate a raw SSE frame, returning the tagged frame or null when it
// must be dropped. Exported for the demux test.
export function parseWorkspaceFrame(data: string): WorkspaceFrame | null {
  let parsed: unknown;
  try {
    parsed = JSON.parse(data);
  } catch {
    return null;
  }
  if (!parsed || typeof parsed !== "object") return null;
  const kind = (parsed as { kind?: unknown }).kind;
  if (typeof kind !== "string") return null;
  if (kind === FRAME_KIND.HELLO) {
    const fleetIds = (parsed as { fleet_ids?: unknown }).fleet_ids;
    if (
      !Array.isArray(fleetIds) ||
      !fleetIds.every((value) => typeof value === "string" && value.length > 0)
    ) {
      return null;
    }
    return parsed as WorkspaceControlFrame;
  }
  if (kind === FRAME_KIND.CATCHING_UP) {
    const dropped = (parsed as { dropped?: unknown }).dropped;
    if (typeof dropped !== "number" || !Number.isSafeInteger(dropped) || dropped < 0) return null;
    return parsed as WorkspaceControlFrame;
  }
  const fleetId = (parsed as { fleet_id?: unknown }).fleet_id;
  if (typeof fleetId !== "string" || fleetId.length === 0) return null;
  return parsed as WorkspaceLiveFrame;
}

function onFrame(entry: Entry, e: MessageEvent): void {
  const frame = parseWorkspaceFrame(typeof e.data === "string" ? e.data : "");
  if (frame === null) return; // malformed / untagged — dropped, never routed
  if (isWorkspaceFrame(frame)) {
    for (const l of entry.workspaceListeners) l(frame);
    if (frame.kind === FRAME_KIND.CATCHING_UP) void backfillGap(entry);
    return;
  }
  const listeners = entry.fleetListeners.get(frame.fleet_id);
  if (!listeners) return; // a fleet no tile is currently watching
  for (const l of listeners) l(frame);
}

function isWorkspaceFrame(frame: WorkspaceFrame): frame is WorkspaceControlFrame {
  return frame.kind === FRAME_KIND.HELLO || frame.kind === FRAME_KIND.CATCHING_UP;
}

function onEventSourceError(entry: Entry): void {
  entry.eventSource?.close();
  entry.eventSource = null;
  setStatus(entry, WORKSPACE_CONNECTION_STATUS.RECONNECTING);
  entry.reconnectAttempts += 1;
  const delayMs = Math.min(
    RECONNECT_BACKOFF_BASE_MS *
      2 ** Math.min(entry.reconnectAttempts, RECONNECT_MAX_BACKOFF_ATTEMPTS),
    RECONNECT_BACKOFF_CAP_MS,
  );
  entry.reconnectTimer = setTimeout(() => {
    entry.reconnectTimer = null;
    startEventSource(entry);
  }, delayMs);
}

function teardown(entry: Entry): void {
  if (entry.reconnectTimer) clearTimeout(entry.reconnectTimer);
  if (entry.idleTimer) clearTimeout(entry.idleTimer);
  entry.eventSource?.close();
  REGISTRY.delete(entry.workspaceId);
}

function createEntry(workspaceId: string, backfill: BackfillFn | null): Entry {
  return {
    workspaceId,
    eventSource: null,
    status: WORKSPACE_CONNECTION_STATUS.CONNECTING,
    fleetListeners: new Map(),
    workspaceListeners: new Set(),
    statusListeners: new Set(),
    refCount: 0,
    reconnectTimer: null,
    reconnectAttempts: 0,
    idleTimer: null,
    hasConnectedOnce: false,
    serverSinceMs: null,
    backfillInFlight: false,
    backfill,
  };
}

function ensureEntry(workspaceId: string, backfill: BackfillFn | null): Entry {
  let entry = REGISTRY.get(workspaceId);
  if (!entry) {
    entry = createEntry(workspaceId, backfill);
    REGISTRY.set(workspaceId, entry);
    startEventSource(entry);
  }
  if (backfill !== null) entry.backfill = backfill;
  if (entry.idleTimer) {
    clearTimeout(entry.idleTimer);
    entry.idleTimer = null;
  }
  return entry;
}

// A tile subscribes for one fleet's frames on the workspace connection. The
// first subscriber for the workspace opens the connection; the last one to
// leave releases it (after an idle grace). Returns an unsubscribe fn.
export function subscribeFleet(
  workspaceId: string,
  fleetId: string,
  listener: FleetFrameListener,
  backfill: BackfillFn | null = null,
): () => void {
  const entry = ensureEntry(workspaceId, backfill);
  let set = entry.fleetListeners.get(fleetId);
  if (!set) {
    set = new Set();
    entry.fleetListeners.set(fleetId, set);
  }
  set.add(listener);
  const subscribedSet = set;
  entry.refCount += 1;
  return () => releaseFleet(workspaceId, fleetId, listener, subscribedSet);
}

export function subscribeWorkspaceFrames(
  workspaceId: string,
  listener: WorkspaceFrameListener,
  backfill: BackfillFn | null = null,
): () => void {
  const entry = ensureEntry(workspaceId, backfill);
  entry.workspaceListeners.add(listener);
  entry.refCount += 1;
  return () => releaseWorkspaceFrameListener(workspaceId, listener);
}

// Observe the workspace connection's health (for the wall's "catching up" /
// degraded eyebrow). Shares the same refcount/idle lifecycle as a fleet
// subscription so a status-only observer still keeps the connection alive.
export function subscribeStatus(
  workspaceId: string,
  listener: StatusListener,
  backfill: BackfillFn | null = null,
): () => void {
  const entry = ensureEntry(workspaceId, backfill);
  entry.statusListeners.add(listener);
  entry.refCount += 1;
  listener(entry.status);
  return () => releaseStatus(workspaceId, listener);
}

function releaseFleet(
  workspaceId: string,
  fleetId: string,
  listener: FleetFrameListener,
  listeners: Set<FleetFrameListener>,
): void {
  const entry = REGISTRY.get(workspaceId);
  if (!entry) return;
  listeners.delete(listener);
  if (listeners.size === 0) entry.fleetListeners.delete(fleetId);
  decRef(entry);
}

function releaseStatus(workspaceId: string, listener: StatusListener): void {
  const entry = REGISTRY.get(workspaceId);
  if (!entry) return;
  entry.statusListeners.delete(listener);
  decRef(entry);
}

function releaseWorkspaceFrameListener(workspaceId: string, listener: WorkspaceFrameListener): void {
  const entry = REGISTRY.get(workspaceId);
  if (!entry) return;
  entry.workspaceListeners.delete(listener);
  decRef(entry);
}

function decRef(entry: Entry): void {
  entry.refCount -= 1;
  if (entry.refCount > 0) return;
  entry.idleTimer = setTimeout(() => teardown(entry), IDLE_RELEASE_MS);
}

export function getWorkspaceConnectionStatus(workspaceId: string): WorkspaceConnectionStatus {
  return REGISTRY.get(workspaceId)?.status ?? WORKSPACE_CONNECTION_STATUS.CONNECTING;
}

// Advance the backfill anchor as the wall confirms server frame times (from the
// SSR seed or a completed backfill page). Never advanced by client-clock live
// frames, so a failed backfill cannot seal the gap it left.
export function noteServerFrameTime(workspaceId: string, createdAtMs: number): void {
  const entry = REGISTRY.get(workspaceId);
  if (!entry) return;
  if (entry.serverSinceMs === null || createdAtMs > entry.serverSinceMs) {
    entry.serverSinceMs = createdAtMs;
  }
}

// Test surface — vitest must reset the module registry between tests; nothing in
// production calls this.
export function __resetWorkspaceRegistryForTests(): void {
  for (const entry of REGISTRY.values()) teardown(entry);
}

// Re-exported so a consumer can narrow a frame's payload after demux.
export type { LiveFrame, WorkspaceLiveFrame };
