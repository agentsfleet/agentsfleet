import React from "react";
import { afterEach, beforeEach, vi } from "vitest";
import { cleanup, render } from "@testing-library/react";

import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";
import { GUIDANCE, OUTCOME, outcomeFor } from "@/lib/events/event-summary";
import { __resetFleetDeliveryFailuresForTests } from "@/components/domain/useFleetDeliveryFailure";

// ── Hoisted mocks ────────────────────────────────────────────────────────

const {
  routerRefreshMock,
  steerFleetActionMock,
  useFleetEventStreamMock,
  capturedOnNew,
  capturedRetry,
} = vi.hoisted(() => ({
  routerRefreshMock: vi.fn(),
  steerFleetActionMock: vi.fn(),
  useFleetEventStreamMock: vi.fn(),
  // Capture the `onNew` callback wired into the external-store runtime so a
  // test can drive it with content the composer UI never emits (e.g. an
  // image-only append) to reach `extractMessageText`'s no-text-part path.
  capturedOnNew: {
    current: null as ((msg: AppendMessage) => Promise<void>) | null,
  },
  capturedRetry: { current: null as (() => void) | null },
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ refresh: routerRefreshMock }),
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/fleets/actions", () => ({
  steerFleetAction: steerFleetActionMock,
}));

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

vi.mock("@/components/domain/SteerComposer", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/domain/SteerComposer")
  >("@/components/domain/SteerComposer");
  return {
    ...actual,
    SteerComposer: (
      props: React.ComponentProps<typeof actual.SteerComposer>,
    ) => {
      capturedRetry.current = props.onRetry;
      return React.createElement(actual.SteerComposer, props);
    },
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
export const ZID = "zomb_test";
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
    outcome: over.outcome ?? OUTCOME.NO_REPLY,
    failureLabel: over.failureLabel ?? null,
    failureDetail: over.failureDetail ?? null,
    createdAt: over.createdAt ?? new Date(Date.UTC(2026, 4, 15, 9, 0, 0)),
    status: over.status ?? "processed",
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
        reply: e.reply,
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
  markOptimisticFailed?: ReturnType<typeof vi.fn>;
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
    markOptimisticFailed: opts?.markOptimisticFailed ?? vi.fn(),
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
    fleetName: FLEET_NAME,
    initial,
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
  steerFleetActionMock.mockReset();
  useFleetEventStreamMock.mockReset();
  // The delivery-failure registry is module-scoped by design (it survives
  // remounts); without this reset a failure recorded in one test leaks a
  // Retry banner — and its stale message — into the next.
  __resetFleetDeliveryFailuresForTests();
  capturedOnNew.current = null;
  capturedRetry.current = null;
});

afterEach(() => cleanup());

export { routerRefreshMock, steerFleetActionMock, useFleetEventStreamMock, capturedOnNew, capturedRetry };
