import React from "react";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import { GUIDANCE, OUTCOME, outcomeFor } from "@/lib/events/event-summary";
import { __resetPendingSendsForTests } from "@/lib/streaming/pending-sends";

// ── Hoisted mocks ────────────────────────────────────────────────────────

const TEST_SUBJECT = "user_fleet_thread";

const {
  routerRefreshMock,
  postSteerMock,
  useFleetEventStreamMock,
  capturedOnNew,
  capturedRun,
  signedIn,
  threadPath,
} = vi.hoisted(() => ({
  // The page the thread renders on, as `usePathname` reports it.
  threadPath: "/w/ws_thread/fleets/fleet_thread",
  routerRefreshMock: vi.fn(),
  postSteerMock: vi.fn(),
  useFleetEventStreamMock: vi.fn(),
  // Capture the `onNew` callback wired into the external-store runtime so a
  // test can drive it with content the composer UI never emits (e.g. an
  // image-only append) to reach `extractMessageText`'s no-text-part path.
  capturedOnNew: {
    current: null as ((msg: AppendMessage) => Promise<void>) | null,
  },
  // What the thread told the runtime last: whether a reply runs, and the
  // queue every send goes through.
  capturedRun: { isRunning: false as boolean | undefined, hasQueue: false },
  // Who the client's auth script says is signed in: null until it loads.
  signedIn: { userId: "user_fleet_thread" as string | null },
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefreshMock }),
  usePathname: () => threadPath,
}));

// The thread keys its pending-send ledger by the signed-in user; the suite
// signs in one fixed person.
vi.mock("@/lib/auth/client", () => ({
  useCurrentUser: () => ({
    isLoaded: signedIn.userId !== null,
    isSignedIn: signedIn.userId !== null,
    userId: signedIn.userId,
    emailAddress: null,
    hasImage: false,
  }),
}));

// The thread's steers leave through the browser transport; the Server Action
// module stays mocked so no server-only import loads under the thread.
vi.mock("@/lib/api/fleet-steer", () => ({ postSteer: postSteerMock }));
vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({}));

vi.mock("@assistant-ui/react", async () => {
  const actual = await vi.importActual<typeof import("@assistant-ui/react")>(
    "@assistant-ui/react",
  );
  return {
    ...actual,
    useExternalStoreRuntime: (
      cfg: Parameters<typeof actual.useExternalStoreRuntime>[0],
    ) => {
      capturedOnNew.current = cfg.onNew ?? null;
      capturedRun.isRunning = cfg.isRunning;
      capturedRun.hasQueue = cfg.queue !== undefined;
      return actual.useExternalStoreRuntime(cfg);
    },
  };
});

vi.mock("@/components/domain/useFleetEventStream", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/domain/useFleetEventStream")
  >("@/components/domain/useFleetEventStream");
  return {
    ...actual,
    useFleetEventStream: useFleetEventStreamMock,
  };
});

import { FleetThread } from "@/components/domain/FleetThread";
import type { EventDetail, EventRow } from "@/lib/api/events";
import {
  CONNECTION_STATUS,
  type FleetEvent,
} from "@/components/domain/useFleetEventStream";

// ── Fixture builders ─────────────────────────────────────────────────────

export const WS = "ws_test";
export const SUBJECT = TEST_SUBJECT;
/** Whom the client's auth script reports; set `userId` to null for "not loaded yet". */
export const clientUser = signedIn;
export const ZID = "zomb_test";
export const THREAD_PATH = threadPath;
export const FLEET_NAME = "github-pr-reviewer";

export function ev(
  over: Partial<FleetEvent> & { actor: string; role: FleetEvent["role"] },
): FleetEvent {
  return {
    id: over.id ?? `e_${Math.random().toString(36).slice(2, 8)}`,
    role: over.role,
    actor: over.actor,
    text: over.text ?? "",
    reply: over.reply ?? "",
    reasoning: over.reasoning,
    thinking: over.thinking,
    reasoningStartedAtMs: over.reasoningStartedAtMs,
    reasoningEndedAtMs: over.reasoningEndedAtMs,
    tools: over.tools,
    replyRecovering: over.replyRecovering,
    outcome: over.outcome ?? OUTCOME.COMPLETED,
    failureLabel: over.failureLabel ?? null,
    failureDetail: over.failureDetail ?? null,
    createdAt: over.createdAt ?? new Date(Date.UTC(2026, 4, 15, 9, 0, 0)),
    status: over.status ?? "processed",
    clientTimestamp: over.clientTimestamp,
    submittedAtMs: over.submittedAtMs,
    custom: over.custom,
  };
}

export function toThreadMessage(e: FleetEvent): ThreadMessageLike {
  return {
    role: e.role,
    id: e.id,
    createdAt: e.createdAt,
    content: [{ type: "text", text: e.text }],
    metadata: {
      custom: {
        actor: e.actor,
        requestJson: e.custom?.requestJson,
        status: e.status,
        queued: e.clientTimestamp === true,
        submittedAtMs: e.submittedAtMs,
        replyRecovering: e.replyRecovering,
        outcome: e.outcome,
        failureLabel: e.failureLabel,
        failureDetail: e.failureDetail,
      },
    },
  };
}

export type StreamMockOverrides = {
  events?: FleetEvent[];
  isRunning?: boolean;
  connectionStatus?: (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];
  appendOptimistic?: ReturnType<typeof vi.fn>;
  reconcileOptimistic?: ReturnType<typeof vi.fn>;
  discardOptimistic?: ReturnType<typeof vi.fn>;
  retryConnection?: ReturnType<typeof vi.fn>;
};

export function mockStream(
  events: FleetEvent[],
  opts?: Omit<StreamMockOverrides, "events">,
) {
  useFleetEventStreamMock.mockReturnValue({
    events,
    connectionStatus: opts?.connectionStatus ?? CONNECTION_STATUS.LIVE,
    isRunning: opts?.isRunning ?? false,
    appendOptimistic:
      opts?.appendOptimistic ?? vi.fn().mockReturnValue("temp_1"),
    reconcileOptimistic: opts?.reconcileOptimistic ?? vi.fn(),
    discardOptimistic: opts?.discardOptimistic ?? vi.fn(),
    retryConnection: opts?.retryConnection ?? vi.fn(),
    convertEvent: toThreadMessage,
  });
}

export function appendMessage(text: string): AppendMessage {
  return {
    role: "user",
    content: [{ type: "text", text }],
    createdAt: new Date(0),
    metadata: { custom: {} },
    parentId: null,
    sourceId: null,
    runConfig: undefined,
  };
}

export function threadElement(initial: EventRow[] = []) {
  return React.createElement(FleetThread, {
    workspaceId: WS,
    fleetId: ZID,
    senderLabel: FLEET_NAME,
    initial,
    viewer: TEST_SUBJECT,
  });
}

export function renderThread() {
  return renderThreadWithInitial([]);
}

export function renderThreadWithInitial(initial: EventRow[]) {
  return render(threadElement(initial));
}

export function serverEvent(over: Partial<EventDetail> = {}): EventDetail {
  const now = Date.UTC(2026, 4, 15, 9, 0, 0);
  return {
    event_id: "event-server-terminal",
    fleet_id: ZID,
    workspace_id: WS,
    actor: "fleet",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "done",
    tokens: 1,
    wall_ms: 10,
    failure_label: null,
    failure_detail: null,
    checkpoint_id: null,
    resumes_event_id: null,
    cost_nanos: 1,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

beforeEach(() => {
  routerRefreshMock.mockReset();
  postSteerMock.mockReset();
  useFleetEventStreamMock.mockReset();
  // The pending-send ledger is module-scoped and storage-mirrored by design
  // (it survives remounts and reloads); without this reset an entry recorded
  // in one test leaks its notice — and its restored text — into the next.
  __resetPendingSendsForTests();
  capturedOnNew.current = null;
  capturedRun.isRunning = false;
  capturedRun.hasQueue = false;
  signedIn.userId = TEST_SUBJECT;
});

afterEach(() => cleanup());

export { routerRefreshMock, postSteerMock, useFleetEventStreamMock, capturedOnNew, capturedRun };
