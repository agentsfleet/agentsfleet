import { afterEach, beforeEach, vi } from "vitest";

import type {
  EventRow,
  WorkspaceControlFrame,
  WorkspaceHelloFrame,
  WorkspaceLiveFrame,
} from "@/lib/api/events";
import type { BackfillOutcome, WorkspaceBackfillRequest } from "@/lib/streaming/fleet-stream-backfill";

import { FRAME_KIND } from "@/lib/api/events-types";
import { HEADLINE } from "@/lib/events/event-payload";

// The wall's store is proven against captured subscriptions rather than a fake
// EventSource: the claims are about what the store does with a frame it was
// handed, and the transport that hands it over has its own suite.
type FrameListener = (frame: WorkspaceLiveFrame) => void;
type ControlListener = (frame: WorkspaceControlFrame) => void;
type BackfillFn = (workspaceId: string, anchorMs: number | null) => void;

const wire = vi.hoisted(() => ({
  fleetListeners: new Map<string, FrameListener>(),
  controlListener: null as ControlListener | null,
  backfill: null as BackfillFn | null,
  noteServerFrameTime: vi.fn(),
  lastGreeting: vi.fn<(workspaceId: string) => WorkspaceHelloFrame | null>(() => null),
  warnBackfillFailure: vi.fn(),
  runWorkspaceBackfill: vi.fn<(req: WorkspaceBackfillRequest) => Promise<BackfillOutcome>>(),
}));

vi.mock("@/lib/streaming/workspace-stream", () => ({
  WORKSPACE_CONNECTION_STATUS: { CONNECTING: "connecting", LIVE: "live", RECONNECTING: "reconnecting", REVOKED: "revoked" },
  noteServerFrameTime: (...a: unknown[]) => wire.noteServerFrameTime(...a),
  lastGreeting: (workspaceId: string) => wire.lastGreeting(workspaceId),
  subscribeStatus: (_workspaceId: string, _listener: unknown, onReconnect: BackfillFn) => {
    wire.backfill = onReconnect;
    return () => {};
  },
  subscribeWorkspaceFrames: (_workspaceId: string, listener: ControlListener) => {
    wire.controlListener = listener;
    return () => {};
  },
  subscribeFleet: (_workspaceId: string, fleetId: string, listener: FrameListener) => {
    wire.fleetListeners.set(fleetId, listener);
    return () => {};
  },
}));

vi.mock("@/lib/streaming/fleet-stream-backfill", () => ({
  runWorkspaceBackfill: (req: WorkspaceBackfillRequest) => wire.runWorkspaceBackfill(req),
  warnBackfillFailure: (...a: unknown[]) => wire.warnBackfillFailure(...a),
}));

export const WORKSPACE_ID = "ws_store";
export const CREATED_AT_MS = 1_700_000_000_000;
// A webhook trigger renders a neutral headline, so a row's feed line is not
// blank and a fresh blank row shows up as a change.
const TRIGGER_ACTOR = "webhook:github";
const TRIGGER_TYPE = "push";
export const TRIGGER_FEED = `${TRIGGER_TYPE} ${HEADLINE.RECEIVED_SUFFIX}`;
/** The feed line of a row a chunk opened: it carries no trigger. */
export const BLANK_FEED = "";
const SETTLED = "processed";

export const noteServerFrameTime = wire.noteServerFrameTime;
export const lastGreeting = wire.lastGreeting;
export const warnBackfillFailure = wire.warnBackfillFailure;
export const runWorkspaceBackfill = wire.runWorkspaceBackfill;

export function received(fleetId: string, eventId: string, counters: object = {}): WorkspaceLiveFrame {
  return {
    kind: FRAME_KIND.EVENT_RECEIVED,
    fleet_id: fleetId,
    event_id: eventId,
    actor: TRIGGER_ACTOR,
    event_type: TRIGGER_TYPE,
    created_at: CREATED_AT_MS,
    ...counters,
  };
}

export function completed(fleetId: string, eventId: string, counters: object = {}): WorkspaceLiveFrame {
  return {
    kind: FRAME_KIND.EVENT_COMPLETE,
    fleet_id: fleetId,
    event_id: eventId,
    status: SETTLED,
    created_at: CREATED_AT_MS,
    updated_at: CREATED_AT_MS,
    ...counters,
  };
}

export function chunk(fleetId: string, eventId: string, text = "…"): WorkspaceLiveFrame {
  return { kind: FRAME_KIND.CHUNK, fleet_id: fleetId, event_id: eventId, text };
}

export function toolStarted(fleetId: string, eventId: string, name: string): WorkspaceLiveFrame {
  return { kind: FRAME_KIND.TOOL_CALL_STARTED, fleet_id: fleetId, event_id: eventId, name, args_redacted: null };
}

/** A settled durable row, as the backfill list serves one. */
export function listRow(fleetId: string, eventId: string, createdAtMs = CREATED_AT_MS): EventRow {
  return {
    event_id: eventId,
    fleet_id: fleetId,
    workspace_id: WORKSPACE_ID,
    actor: TRIGGER_ACTOR,
    event_type: TRIGGER_TYPE,
    status: SETTLED,
    tokens: null,
    wall_ms: null,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: null,
    created_at: createdAtMs,
    updated_at: createdAtMs,
  };
}

export function push(fleetId: string, frame: WorkspaceLiveFrame) {
  const listener = wire.fleetListeners.get(fleetId);
  if (!listener) throw new Error(`no subscription for ${fleetId}`);
  listener(frame);
}

export function greet(frame: WorkspaceControlFrame) {
  if (!wire.controlListener) throw new Error("no workspace subscription");
  wire.controlListener(frame);
}

export function reconnectBackfill(anchorMs: number | null = null) {
  if (!wire.backfill) throw new Error("no status subscription");
  wire.backfill(WORKSPACE_ID, anchorMs);
}

let queuedFrame: FrameRequestCallback | null = null;

/** Runs the one notification flush the store coalesced into the next animation frame. */
export function flushFrame() {
  const callback = queuedFrame;
  queuedFrame = null;
  callback?.(0);
}

/** Captured subscriptions, a manual animation frame, and clean spies around every test. */
export function setupWorkspaceWire() {
  beforeEach(() => {
    wire.fleetListeners.clear();
    wire.controlListener = null;
    wire.backfill = null;
    queuedFrame = null;
    vi.stubGlobal(
      "requestAnimationFrame",
      vi.fn((callback: FrameRequestCallback) => {
        queuedFrame = callback;
        return 1;
      }),
    );
    vi.stubGlobal("cancelAnimationFrame", vi.fn());
  });
  afterEach(() => {
    vi.unstubAllGlobals();
    wire.runWorkspaceBackfill.mockReset();
    wire.noteServerFrameTime.mockReset();
    wire.lastGreeting.mockReset();
    wire.lastGreeting.mockReturnValue(null);
    wire.warnBackfillFailure.mockReset();
  });
}
