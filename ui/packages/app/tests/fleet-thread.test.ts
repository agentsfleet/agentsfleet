import React from "react";
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

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
} =
  vi.hoisted(() => ({
    routerRefreshMock: vi.fn(),
    steerFleetActionMock: vi.fn(),
    useFleetEventStreamMock: vi.fn(),
    // Capture the `onNew` callback wired into the external-store runtime so a
    // test can drive it with content the composer UI never emits (e.g. an
    // image-only append) to reach `extractMessageText`'s no-text-part path.
    capturedOnNew: { current: null as ((msg: AppendMessage) => Promise<void>) | null },
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
    useExternalStoreRuntime: (cfg: Parameters<typeof actual.useExternalStoreRuntime>[0]) => {
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
  const actual = await vi.importActual<typeof import("@/components/domain/SteerComposer")>(
    "@/components/domain/SteerComposer",
  );
  return {
    ...actual,
    SteerComposer: (props: React.ComponentProps<typeof actual.SteerComposer>) => {
      capturedRetry.current = props.onRetry;
      return React.createElement(actual.SteerComposer, props);
    },
  };
});

import { FleetThread } from "@/components/domain/FleetThread";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";
import type { EventRow } from "@/lib/api/events";
import {
  CONNECTION_STATUS,
  type FleetEvent,
} from "@/components/domain/useFleetEventStream";

// ── Fixture builders ─────────────────────────────────────────────────────

const WS = "ws_test";
const ZID = "zomb_test";
const FLEET_NAME = "github-pr-reviewer";

function ev(over: Partial<FleetEvent> & { actor: string; role: FleetEvent["role"] }): FleetEvent {
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

function toThreadMessage(e: FleetEvent): ThreadMessageLike {
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

type StreamMockOverrides = {
  events?: FleetEvent[];
  isRunning?: boolean;
  connectionStatus?: (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];
  appendOptimistic?: ReturnType<typeof vi.fn>;
  reconcileOptimistic?: ReturnType<typeof vi.fn>;
  markOptimisticFailed?: ReturnType<typeof vi.fn>;
  discardOptimistic?: ReturnType<typeof vi.fn>;
  retryConnection?: ReturnType<typeof vi.fn>;
};

function mockStream(events: FleetEvent[], opts?: Omit<StreamMockOverrides, "events">) {
  useFleetEventStreamMock.mockReturnValue({
    events,
    connectionStatus: opts?.connectionStatus ?? CONNECTION_STATUS.LIVE,
    isRunning: opts?.isRunning ?? false,
    appendOptimistic: opts?.appendOptimistic ?? vi.fn().mockReturnValue("temp_1"),
    reconcileOptimistic: opts?.reconcileOptimistic ?? vi.fn(),
    markOptimisticFailed: opts?.markOptimisticFailed ?? vi.fn(),
    discardOptimistic: opts?.discardOptimistic ?? vi.fn(),
    retryConnection: opts?.retryConnection ?? vi.fn(),
    convertEvent: toThreadMessage,
  });
}

function appendMessage(text: string): AppendMessage {
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

function threadElement(initial: EventRow[] = []) {
  return React.createElement(FleetThread, {
    workspaceId: WS,
    fleetId: ZID,
    fleetName: FLEET_NAME,
    initial,
  });
}

function renderThread() {
  return renderThreadWithInitial([]);
}

function renderThreadWithInitial(initial: EventRow[]) {
  return render(
    React.createElement(FleetThread, {
      workspaceId: WS,
      fleetId: ZID,
      fleetName: FLEET_NAME,
      initial,
    }),
  );
}

function serverEvent(over: Partial<EventRow> = {}): EventRow {
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

// ── FleetThread integration ─────────────────────────────────────────────

describe("FleetThread — empty state", () => {
  it("renders the waiting-for-activity hint when no events", () => {
    mockStream([]);
    renderThread();
    expect(screen.getByText(/Message this fleet or wait for its next trigger/i)).toBeTruthy();
    expect(screen.queryByText(/0 events/i)).toBeNull();
  });
});

describe("FleetThread — header chrome", () => {
  it("shows one Chat heading and one aligned Live indicator", () => {
    mockStream([
      ev({ role: "system", actor: "config_reload", text: "Reloaded" }),
    ]);
    const { container } = renderThread();
    const header = container.querySelector('[data-testid="fleet-chat-header"]');
    expect(header).toBeTruthy();
    expect(header?.className).toMatch(/justify-between/);
    expect(screen.getByRole("heading", { name: "Chat" })).toBeTruthy();
    expect(screen.queryByRole("link", { name: "Steer" })).toBeNull();
    expect(screen.queryByText(/1 events/)).toBeNull();
    const liveStatus = screen.getByLabelText("Connection status: Live");
    expect(liveStatus.className).toMatch(/text-pulse/);
    expect(liveStatus.querySelector('[aria-hidden="true"]')?.className)
      .toMatch(/bg-current/);
  });

  it("uses one destructive colour for the Offline label and dot", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.OFFLINE });
    renderThread();
    const offlineStatus = screen.getByLabelText("Connection status: Not live");
    expect(offlineStatus.className).toMatch(/text-destructive/);
    expect(offlineStatus.querySelector('[aria-hidden="true"]')?.className)
      .toMatch(/bg-current/);
  });

  it("keeps the composer in the static transcript footer", () => {
    mockStream([]);
    const { container } = renderThread();
    const transcript = container.querySelector('[aria-label="Fleet chat"]');
    const composer = container.querySelector('[aria-label="Chat composer"]');
    expect(transcript).toBeTruthy();
    expect(composer).toBeTruthy();
    expect(transcript?.contains(composer)).toBe(true);
    expect(composer?.getAttribute("id")).toBe("fleet-steer-composer");
    const footer = container.querySelector('[data-testid="fleet-chat-footer"]');
    expect(footer?.contains(composer)).toBe(true);
    expect(footer?.className).toMatch(/max-w-6xl/);
    expect(footer?.className).toMatch(/shrink-0/);
    expect(container.querySelector('[role="log"]')?.contains(composer)).toBe(false);
    expect(screen.getByRole("button", { name: /jump to latest/i }).className).toMatch(/absolute/);
  });

  it("names each connection state rather than only the live one", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    renderThread();
    expect(screen.getByText(/^Reconnecting…$/)).toBeTruthy();
  });

  it("says a lost feed is not live, and keeps the composer usable", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.OFFLINE });
    renderThread();
    expect(screen.getByText(/^Not live$/)).toBeTruthy();
    const input = screen.getByPlaceholderText(/message this fleet/i) as HTMLTextAreaElement;
    expect(input.disabled).toBe(false);
  });
});

describe("FleetThread — summary refresh", () => {
  it("does not refresh for terminal events already present in the server snapshot", () => {
    mockStream([]);
    renderThreadWithInitial([serverEvent()]);

    expect(routerRefreshMock).not.toHaveBeenCalled();
  });

  it("refreshes server-rendered summaries once when a live event completes", async () => {
    const received = ev({
      id: "event-refresh",
      role: "assistant",
      actor: "fleet",
      status: "received",
    });
    mockStream([received], { isRunning: true });
    const view = renderThread();
    expect(routerRefreshMock).not.toHaveBeenCalled();

    mockStream([{ ...received, status: "processed" }]);
    view.rerender(
      React.createElement(FleetThread, { workspaceId: WS, fleetId: ZID, fleetName: FLEET_NAME, initial: [] }),
    );

    await waitFor(() => expect(routerRefreshMock).toHaveBeenCalledTimes(1));
    view.rerender(
      React.createElement(FleetThread, { workspaceId: WS, fleetId: ZID, fleetName: FLEET_NAME, initial: [] }),
    );
    expect(routerRefreshMock).toHaveBeenCalledTimes(1);
  });
});

describe("FleetThread — role rendering", () => {
  it("renders an operator steer and the fleet's reply as separate conversation turns", () => {
    // A durable event can contain both the trigger and its response. The
    // transcript normalizes those into two assistant-ui message roots so
    // alignment, scrolling, and future actions remain message-scoped.
    mockStream([
      ev({
        role: "user",
        actor: "steer:user_abc",
        text: "please review PR 517",
        reply: "Reviewed. Two suggestions, Continuous Integration (CI) passing.",
        status: "processed",
      }),
    ]);
    const { container } = renderThread();
    // The operator's question survives (the old code dropped it for the reply).
    expect(screen.getByText(/please review PR 517/)).toBeTruthy();
    // The fleet's reply survives too, and it is NOT attributed to the operator.
    expect(screen.getByText(/Reviewed\. Two suggestions/)).toBeTruthy();
    expect(screen.getByText("Operator")).toBeTruthy();
    expect(screen.getAllByText(FLEET_NAME).length).toBeGreaterThanOrEqual(1);
    expect(container.querySelectorAll("[data-message-id]")).toHaveLength(2);
    // The reply sits under the fleet's chip, not the operator's.
    const replyRow = screen.getByText(/Reviewed\. Two suggestions/).closest("[data-role]");
    expect(replyRow?.getAttribute("data-role")).toBe("assistant");
  });

  it("shows the outcome as the fleet's bubble when a turn completes with no reply", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:user_abc",
        text: "deploy staging",
        reply: "",
        status: "gate_blocked",
        outcome: OUTCOME.WAITING_APPROVAL,
      }),
    ]);
    renderThread();
    expect(screen.getByText(/deploy staging/)).toBeTruthy();
    // A blocked turn does not render the instruction as if it succeeded — the
    // fleet bubble states the outcome (coding-agent finding #4).
    expect(screen.getByText(OUTCOME.WAITING_APPROVAL)).toBeTruthy();
  });

  it("shows motion while connecting, and no band above the conversation", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const { container } = renderThread();

    // The one moment the operator wants a sign of life is while we are
    // trying, so the dot moves — and a state with no decision attached does
    // not earn a band above their conversation.
    expect(container.querySelector('[data-connection="connecting"]')).toBeTruthy();
    expect(container.querySelector(".animate-pulse")).toBeTruthy();
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
    expect(screen.queryByText(/History remains available/i)).toBeNull();
  });

  it("keeps a band only for a connection that asks the operator to decide", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    const reconnecting = renderThread();
    expect(screen.queryByTestId("fleet-connection-notice")).toBeNull();
    reconnecting.unmount();

    mockStream([], { connectionStatus: CONNECTION_STATUS.OFFLINE });
    renderThread();
    // Nothing to wait for and a choice to make — so it speaks, and offers it.
    expect(screen.getByTestId("fleet-connection-notice")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy();
  });

  it("announces arrival once, and only when it was actually waiting", async () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const view = renderThread();
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    await act(async () => { view.rerender(threadElement()); });

    const indicator = view.container.querySelector('[data-connection="live"]');
    expect(indicator?.getAttribute("data-arrived")).toBe("true");
    view.unmount();

    // A surface that mounts already-live announces nothing: there was no
    // wait to resolve.
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    const fresh = renderThread();
    expect(
      fresh.container.querySelector('[data-connection="live"]')?.getAttribute("data-arrived"),
    ).toBeNull();
  });

  it("animates a turn that has started but not spoken yet", () => {
    mockStream([
      ev({ role: "user", actor: "steer:user_abc", text: "Howdy", reply: "", status: "received" }),
    ]);
    renderThread();

    // "Still working." reads the same at one second and at five minutes.
    expect(screen.getByTestId("fleet-working")).toBeTruthy();
    expect(screen.queryByText(OUTCOME.WORKING)).toBeNull();
  });

  it("keeps repeated startup failures inline in one expandable activity group", () => {
    const cause = "no instructions configured";
    mockStream(
      Array.from({ length: 15 }, (_, i) =>
        ev({
          id: `fail_${i}`,
          role: "system",
          actor: "webhook:github",
          text: "edited #541",
          reply: "",
          status: "fleet_error",
          outcome: "Failed a startup safety check",
          failureLabel: "startup_posture",
          failureDetail: cause,
        }),
      ),
    );
    renderThread();

    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
    expect(screen.getByTestId("group-count").textContent).toBe("×15");
    expect(screen.getByText(/This fleet needs instructions before it can respond\./)).toBeTruthy();
    expect(screen.queryByText(cause)).toBeNull();
  });

  it("keeps non-actionable repeated failures compact without guidance", () => {
    mockStream(
      Array.from({ length: 3 }, (_, i) =>
        ev({
          id: `oom_${i}`,
          role: "system",
          actor: "webhook:github",
          text: "edited #541",
          reply: "",
          status: "fleet_error",
          outcome: "Ran out of memory",
          failureLabel: "oom_kill",
          failureDetail: null,
        }),
      ),
    );
    renderThread();

    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
    expect(screen.getByTestId("group-count").textContent).toBe("×3");
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("animates a still-working integration delivery instead of stating an outcome", () => {
    // A received (streaming) system row has no settled outcome yet, so the
    // compact tick shows motion, not an outcome clause.
    mockStream([
      ev({
        id: "wk",
        role: "system",
        actor: "webhook:github",
        text: "opened #542",
        reply: "",
        status: "received",
        outcome: OUTCOME.WORKING,
      }),
    ]);
    const { container } = renderThread();

    const tick = container.querySelector('[data-compact="true"]');
    expect(tick?.textContent).toContain("opened #542");
    expect(tick?.textContent).not.toContain(OUTCOME.WORKING);
  });

  it("shows no banner for a single failure, and clears it once the fleet recovers", () => {
    const failure = () =>
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited #541",
        reply: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check",
        failureLabel: "startup_posture",
      });

    mockStream([failure()]);
    const single = renderThread();
    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
    single.unmount();

    // Recovery is the case that matters: a banner that outlived it would
    // report a fleet as broken while it is working.
    mockStream([
      failure(),
      failure(),
      ev({ role: "system", actor: "webhook:github", text: "edited #542", reply: "", status: "processed" }),
    ]);
    renderThread();
    expect(screen.queryByTestId("fleet-failure-banner")).toBeNull();
  });

  it("collapses a run of identical deliveries into one row that opens on demand", async () => {
    const burst = Array.from({ length: 15 }, (_, i) =>
      ev({
        id: `burst_${i}`,
        role: "system",
        actor: "webhook:github",
        text: "edited · agentsfleet/agentsfleet#541",
        reply: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check — no instructions configured",
        failureLabel: "startup_posture",
      }),
    );
    mockStream(burst);
    const { container } = renderThread();

    // Fifteen deliveries, one row — and the count is stated, not implied.
    expect(screen.getByTestId("group-count").textContent).toBe("×15");
    expect(container.querySelectorAll('[data-compact="true"]')).toHaveLength(0);

    // The count is a summary the operator can always check.
    const toggle = screen.getByRole("button", { expanded: false });
    await act(async () => { fireEvent.click(toggle); });
    expect(container.querySelectorAll('[data-compact="true"]')).toHaveLength(15);
  });

  it("keeps each grouped delivery's payload reachable after expansion", async () => {
    const payload = '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541}';
    mockStream([
      ev({
        id: "payload_1",
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        custom: { requestJson: payload },
      }),
      ev({
        id: "payload_2",
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        custom: { requestJson: payload },
      }),
    ]);
    renderThread();

    await act(async () => { fireEvent.click(screen.getByRole("button", { expanded: false })); });
    expect(screen.getAllByText("Details")).toHaveLength(2);
  });

  it("keeps reply-bearing activity as separate conversation turns", async () => {
    // Reply-bearing events are not grouped: each trigger and response needs
    // its own assistant-ui root, even when the trigger text is identical.
    const member = (id: string, reply: string, requestJson?: string) =>
      ev({
        id,
        role: "system",
        actor: "webhook:github",
        text: "edited #541",
        reply,
        status: "processed",
        outcome: OUTCOME.NO_REPLY,
        ...(requestJson ? { custom: { requestJson } } : {}),
      });
    mockStream([
      member("g1", "", '{"action":"opened","repo":"o/r","number":1}'),
      member("g2", "reviewed it", undefined),
      member("g3", "reviewed it", undefined),
    ]);
    const { container } = renderThread();

    expect(screen.queryByTestId("group-count")).toBeNull();
    expect(container.querySelectorAll('[data-compact="true"]')).toHaveLength(3);
    expect(container.querySelectorAll('[data-role="assistant"]')).toHaveLength(2);
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/"repo":\s*"o\/r"/)).toBeTruthy();
  });

  it("links out from an activity delivery that carries only a run URL", () => {
    // A completed-run payload has a link but no repository reference, so the
    // annotation renders a generic source action.
    mockStream([
      ev({
        id: "run1",
        role: "system",
        actor: "webhook:github",
        text: "ci finished",
        reply: "",
        status: "processed",
        outcome: OUTCOME.NO_REPLY,
        custom: {
          requestJson:
            '{"workflow_name":"ci","conclusion":"success","repo":"o/r","run_url":"https://ci.example.test/1"}',
        },
      }),
    ]);
    const { container } = renderThread();
    const link = container.querySelector('a[href^="https://ci.example.test"]');
    expect(link).toBeTruthy();
    expect(container.querySelector('[data-slot="badge"]')).toBeNull();
  });

  it("shows the operator's payload and the failure guidance on a steer that failed startup", () => {
    // One durable turn: the operator steered, the run failed a startup check.
    // The trigger row discloses the submitted payload; the reply row states
    // the outcome and points at the fix.
    mockStream([
      ev({
        id: "steer_fail",
        role: "user",
        actor: "steer:user_abc",
        text: "deploy the review guidelines",
        reply: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check",
        failureLabel: "startup_posture",
        custom: { requestJson: '{"message":"deploy the review guidelines"}' },
      }),
    ]);
    renderThread();
    // The trigger row exposes the operator's own submitted payload.
    expect(screen.getByText("Details")).toBeTruthy();
    // The reply row carries the actionable guidance.
    expect(screen.getByTestId("failure-guidance").textContent).toContain("Tell the fleet what to do in its instructions, then retry.");
    expect(screen.getByRole("link", { name: "Edit instructions" })).toBeTruthy();
  });

  it("streams a fleet reply that has begun but not finished", () => {
    // A reply mid-stream: status received, partial text already accumulated.
    // The reply body shows with the streaming cursor, not the working dots.
    mockStream([
      ev({
        id: "sr",
        role: "assistant",
        actor: "fleet",
        text: "",
        reply: "Half a thought",
        status: "received",
        outcome: OUTCOME.WORKING,
      }),
    ]);
    renderThread();
    expect(screen.getByText(/Half a thought/)).toBeTruthy();
    expect(screen.getByLabelText("streaming")).toBeTruthy();
  });

  it("breaks a group when the operator speaks mid-burst", () => {
    const activity = (id: string) =>
      ev({ id, role: "system", actor: "webhook:github", text: "edited #541", reply: "", status: "fleet_error" });
    mockStream([
      activity("a1"), activity("a2"),
      ev({ role: "user", actor: "steer:user_abc", text: "what is going on?" }),
      activity("b1"), activity("b2"),
    ]);
    const { container } = renderThread();

    // Two groups, and the operator's question still reads as their own row.
    expect(container.querySelectorAll('[data-group="true"]')).toHaveLength(2);
    expect(screen.getByText("what is going on?")).toBeTruthy();
  });

  it("renders an integration delivery as a source-context card with reachable payload", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541 — Fix routing",
        reply: "",
        status: "processed",
        outcome: OUTCOME.NO_REPLY,
        custom: { requestJson: '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541}' },
      }),
    ]);
    const { container } = renderThread();

    // One integration record, not a duplicated source row plus outcome row.
    const tick = container.querySelector('[data-compact="true"]');
    expect(tick).toBeTruthy();
    expect(container.querySelectorAll('[data-role="system"]')).toHaveLength(1);
    expect(tick?.textContent).toContain("agentsfleet/agentsfleet#541");
    // The outcome is readable beneath the source context, not another row.
    expect(screen.getByText(OUTCOME.NO_REPLY).className).toMatch(/text-muted-foreground/);
    // Disclosure remains reachable beside the source evidence.
    expect(screen.getByText("Details")).toBeTruthy();
  });

  it("keeps the operator's and the fleet's own rows in the full skeleton", () => {
    mockStream([
      ev({ role: "user", actor: "steer:user_abc", text: "deploy staging", reply: "" }),
      ev({ role: "assistant", actor: "fleet", text: "", reply: "Deployed." }),
    ]);
    const { container } = renderThread();

    // Regression guard: demotion applies to activity ONLY. A
    // conversation row that lost its chip would be the failure this catches.
    expect(container.querySelector('[data-compact="true"]')).toBeNull();
    expect(container.querySelectorAll("[data-chip]").length).toBeGreaterThan(0);
    expect(screen.getByText("deploy staging")).toBeTruthy();
    expect(screen.getByText("Deployed.")).toBeTruthy();
  });

  it("links the repository reference out when the payload carries a URL", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
    ]);
    const { container } = renderThread();

    expect(screen.getByRole("link", { name: "agentsfleet/agentsfleet#541" })).toBeTruthy();
    expect(screen.getByText("opened")).toBeTruthy();
    const link = container.querySelector('a[href^="https://github.com"]');
    expect(link).toBeTruthy();
    expect(link?.getAttribute("rel")).toContain("noopener");
  });

  it("keeps an activity headline readable when its linked source reference is removed", () => {
    mockStream([
      ev({
        id: "headline_1",
        role: "system",
        actor: "webhook:github",
        text: "opened agentsfleet/agentsfleet#541 — Fix routing",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
      ev({
        id: "headline_2",
        role: "system",
        actor: "webhook:github",
        text: "edited · agentsfleet/agentsfleet#542 — Add evidence",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"edited","repo":"agentsfleet/agentsfleet","number":542,"url":"https://github.com/agentsfleet/agentsfleet/pull/542"}',
        },
      }),
      ev({
        id: "headline_3",
        role: "system",
        actor: "webhook:github",
        text: "closed agentsfleet/agentsfleet#543 after review",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"closed","repo":"agentsfleet/agentsfleet","number":543,"url":"https://github.com/agentsfleet/agentsfleet/pull/543"}',
        },
      }),
      ev({
        id: "headline_4",
        role: "system",
        actor: "webhook:github",
        text: "synchronized agentsfleet/agentsfleet#544",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"synchronized","repo":"agentsfleet/agentsfleet","number":544,"url":"https://github.com/agentsfleet/agentsfleet/pull/544"}',
        },
      }),
    ]);
    renderThread();

    expect(screen.getByText("opened · Fix routing")).toBeTruthy();
    expect(screen.getByText("edited · Add evidence")).toBeTruthy();
    expect(screen.getByText("closed after review")).toBeTruthy();
    expect(screen.getByText("synchronized")).toBeTruthy();
  });

  it("retains an activity headline that does not repeat its linked reference", () => {
    mockStream([
      ev({
        id: "reference_absent",
        role: "system",
        actor: "webhook:github",
        text: "GitHub delivery received",
        reply: "",
        status: "processed",
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
    ]);
    renderThread();

    expect(screen.getByText("GitHub delivery received")).toBeTruthy();
  });

  it("renders no link for a payload whose URL is not an absolute http(s) address", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "opened · agentsfleet/agentsfleet#541",
        reply: "",
        status: "processed",
        // A script URL and a relative path are both refused: one executes,
        // the other resolves against the console's own origin.
        custom: {
          requestJson:
            '{"action":"opened","repo":"agentsfleet/agentsfleet","number":541,"url":"javascript:alert(1)"}',
        },
      }),
    ]);
    const { container } = renderThread();

    expect(screen.getByText("opened · agentsfleet/agentsfleet#541")).toBeTruthy();
    expect(container.querySelector("a[href]")).toBeNull();
  });

  it("names the failing check and what to do about it on a startup failure", () => {
    const cause = "startup check 'instructions' failed: no instructions configured";
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited agentsfleet/agentsfleet#541",
        reply: "",
        status: "fleet_error",
        outcome: outcomeFor({ status: "fleet_error", failure_label: "startup_posture", failure_detail: cause }),
        failureLabel: "startup_posture",
      }),
    ]);
    renderThread();
    // The cause reaches the row, not just the class sentence ...
    expect(screen.getByText(new RegExp(cause))).toBeTruthy();
    // ... and the operator is told where to fix it.
    expect(screen.getByTestId("failure-guidance").textContent).toContain("Tell the fleet what to do in its instructions, then retry.");
    expect(screen.getByRole("link", { name: "Edit instructions" })).toBeTruthy();
  });

  it("offers no guidance for a failure class the operator cannot act on", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited agentsfleet/agentsfleet#541",
        reply: "",
        status: "fleet_error",
        outcome: outcomeFor({ status: "fleet_error", failure_label: "oom_kill", failure_detail: null }),
        failureLabel: "oom_kill",
      }),
    ]);
    renderThread();
    expect(screen.queryByText("Tell the fleet what to do in its instructions, then retry.")).toBeNull();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("lets the fleet's own reply stand instead of canned guidance", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "edited agentsfleet/agentsfleet#541",
        reply: "I recovered on the retry and reviewed the change.",
        status: "fleet_error",
        outcome: OUTCOME.FAILED,
        failureLabel: "startup_posture",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/I recovered on the retry/)).toBeTruthy();
    expect(screen.queryByTestId("failure-guidance")).toBeNull();
  });

  it("labels an operator steer with a word, never the account identifier", () => {
    const accountId = "user_3gkbgxjnujsxbdxttcwcslpc87k";
    mockStream([
      ev({
        role: "user",
        actor: `steer:${accountId}`,
        text: "morning health check",
      }),
    ]);
    const { container } = renderThread();
    expect(screen.getByText(/morning health check/)).toBeTruthy();
    expect(screen.getByText("Operator")).toBeTruthy();
    expect(screen.getByText("OP")).toBeTruthy();
    expect(container.textContent).not.toContain(accountId);
  });

  it("labels a fleet reply with the fleet's own name", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", reply: "reviewed it" })]);
    renderThread();
    expect(screen.getByText(FLEET_NAME)).toBeTruthy();
    expect(screen.getByText("reviewed it")).toBeTruthy();
  });

  it("falls back to 'Fleet' for the reply when no fleet name is known", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", reply: "reviewed it" })]);
    // A console that never learned the fleet's name still labels the reply.
    render(
      React.createElement(FleetThread, {
        workspaceId: WS,
        fleetId: ZID,
        fleetName: "",
        initial: [],
      }),
    );
    expect(screen.getByText("Fleet")).toBeTruthy();
  });

  it("gives every row a sender chip and a machine-readable timestamp", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "reviewed it" })]);
    const { container } = renderThread();
    expect(container.querySelector('[data-chip="fleet"]')).toBeTruthy();
    const stamp = container.querySelector("time");
    expect(stamp?.getAttribute("dateTime")).toBe(
      new Date(Date.UTC(2026, 4, 15, 9, 0, 0)).toISOString(),
    );
  });

  it("renders an assistant reply", () => {
    mockStream([
      ev({ role: "assistant", actor: "fleet", reply: "snapshot taken." }),
    ]);
    renderThread();
    expect(screen.getByText(/snapshot taken/)).toBeTruthy();
  });

  it("renders a system meta-row with the actor as the chip label", () => {
    mockStream([
      ev({
        role: "system",
        actor: "cron",
        text: "tick · */30 * * * * · 09:30 UTC",
      }),
    ]);
    renderThread();
    expect(screen.getByText("Schedule")).toBeTruthy();
    expect(screen.getByText(/tick/)).toBeTruthy();
  });

  it("renders a continuation system row with its chip label", () => {
    mockStream([ev({ role: "system", actor: "continuation", text: "resumed after gate" })]);
    renderThread();
    expect(screen.getByText("Continuation")).toBeTruthy();
    expect(screen.getByText(/resumed after gate/)).toBeTruthy();
  });

  it("renders a gate_blocked system row with its chip label", () => {
    mockStream([ev({ role: "system", actor: "gate_blocked", text: "blocked on approval" })]);
    renderThread();
    expect(screen.getByText("Approval gate")).toBeTruthy();
    expect(screen.getByText(/blocked on approval/)).toBeTruthy();
  });

  it("offers the payload disclosure for a platform identity, not only a prefixed actor", () => {
    // A GitHub App event arrives as the actor `github-app`, not `webhook:…`.
    // Gating the payload on the prefix is why those rows rendered blank.
    mockStream([
      ev({
        role: "system",
        actor: "github-app",
        text: "opened · owner/repo#7",
        custom: { requestJson: '{"repo":"owner/repo"}' },
      }),
    ]);
    renderThread();
    expect(screen.getByText("GitHub App")).toBeTruthy();
    expect(screen.getByText(/opened · owner\/repo#7/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/"repo":\s*"owner\/repo"/)).toBeTruthy();
  });

  it("renders a webhook row with the source tag and collapsible payload", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "workflow_run · main · success",
        custom: { requestJson: '{"action":"completed"}' },
      }),
    ]);
    renderThread();
    expect(screen.getByText("GitHub App")).toBeTruthy();
    expect(screen.getByText(/workflow_run · main · success/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Details" }));
    expect(screen.getByText(/"action":\s*"completed"/)).toBeTruthy();
  });

  it("uses the canonical outcome when a webhook has no response text", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "",
        status: "fleet_error",
        outcome: "Failed a startup safety check",
        failureLabel: "startup_posture",
        custom: { requestJson: "{}" },
      }),
    ]);
    renderThread();
    expect(screen.getByText("GitHub App")).toBeTruthy();
    expect(screen.getByText("This fleet needs instructions before it can respond.")).toBeTruthy();
  });

  it("renders an optimistic user message with the queued badge", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:pending",
        text: "investigate the spike",
        status: "optimistic",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/investigate the spike/)).toBeTruthy();
    expect(screen.getByText(/^sending$/i)).toBeTruthy();
  });

  it("renders a failed user message with the destructive failed badge", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:pending",
        text: "this steer did not land",
        status: "failed",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/this steer did not land/)).toBeTruthy();
    expect(screen.getByText(/^not sent$/i)).toBeTruthy();
    // The in-flight annotation must not also render for a failed row.
    expect(screen.queryByText(/^sending$/i)).toBeNull();
  });

  it("renders a fleet_error as a destructive fleet reply", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        reply: "Provider returned 429; retry budget exhausted",
        status: "fleet_error",
      }),
    ]);
    renderThread();
    expect(screen.queryByText("fleet_error")).toBeNull();
    expect(screen.getByText(/Provider returned 429/)).toBeTruthy();
  });
});

describe("FleetThread — fluid composer", () => {
  it("keeps the message field and its send action available while running", () => {
    mockStream(
      [ev({ role: "assistant", actor: "fleet", text: "streaming…", status: "received" })],
      { isRunning: true },
    );
    renderThread();
    const input = screen.getByPlaceholderText(/message this fleet/i) as HTMLTextAreaElement;
    expect(input.disabled).toBe(false);
    // A working fleet is not a reason to park a message in the browser: the
    // send action stays live and nothing announces a queue.
    expect(screen.getByRole("button", { name: /^Send/ })).toBeTruthy();
    expect(screen.queryByText(/will queue/i)).toBeNull();
  });

  it("uses the idle placeholder when not running", () => {
    mockStream([], { isRunning: false });
    renderThread();
    expect(screen.getByPlaceholderText(/message this fleet/i)).toBeTruthy();
  });
});

describe("FleetThread — steer submission", () => {
  it("ignores Retry when no delivery has failed", () => {
    mockStream([]);
    renderThread();
    expect(capturedRetry.current).toBeTypeOf("function");
    act(() => capturedRetry.current!());
    expect(steerFleetActionMock).not.toHaveBeenCalled();
  });

  it("serialises rapid submissions so their HTTP requests reach the server in order", async () => {
    mockStream([]);
    // Two rapid sends: the first resolves slowly, the second quickly. Without
    // serialisation the fast one's HTTP request would land first and the server would
    // assign it the earlier event id — "stop" before "deploy".
    const order: string[] = [];
    let releaseFirst: (() => void) | null = null;
    steerFleetActionMock.mockImplementationOnce(
      (_ws: string, _z: string, text: string) =>
        new Promise((resolve) => {
          releaseFirst = () => {
            order.push(text);
            resolve({ ok: true, data: { event_id: "evt_1" } });
          };
        }),
    );
    steerFleetActionMock.mockImplementationOnce((_ws: string, _z: string, text: string) => {
      order.push(text);
      return Promise.resolve({ ok: true, data: { event_id: "evt_2" } });
    });
    renderThread();

    const first = capturedOnNew.current!(appendMessage("deploy"));
    const second = capturedOnNew.current!(appendMessage("stop"));
    // The second HTTP request must not fire until the first resolves.
    await waitFor(() => expect(releaseFirst).not.toBeNull());
    expect(order).toEqual([]);
    await act(async () => {
      releaseFirst!();
      await Promise.all([first, second]);
    });
    expect(order).toEqual(["deploy", "stop"]);
  });

  it("calls steerFleetAction and reconciles the optimistic message on ok", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_42");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, markOptimisticFailed });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: true,
      data: { event_id: "evt_real_42" },
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("deploy the canary"));
    await waitFor(() =>
      expect(steerFleetActionMock).toHaveBeenCalledWith(
        WS,
        ZID,
        "deploy the canary",
      ),
    );
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy the canary",
      "steer:pending",
    );
    expect(reconcileOptimistic).toHaveBeenCalledWith("temp_42", "evt_real_42");
    expect(markOptimisticFailed).not.toHaveBeenCalled();
    expect(refreshed).toHaveBeenCalledTimes(1);
    unsubscribe();
  });

  it("accepts a steer that completed before its HTTP response returned", async () => {
    const reconcileOptimistic = vi.fn().mockReturnValue(true);
    mockStream([], { reconcileOptimistic });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: true,
      data: { event_id: "evt_already_complete" },
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("fast completion"));
    expect(reconcileOptimistic).toHaveBeenCalledWith(
      "temp_1",
      "evt_already_complete",
    );
  });

  it("marks the optimistic message failed when the action returns ok:false", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_99");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, markOptimisticFailed });
    steerFleetActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
      errorCode: "UZ-AUTH-401",
    });
    renderThread();
    await capturedOnNew.current!(appendMessage("deploy that fails"));
    await waitFor(() =>
      expect(markOptimisticFailed).toHaveBeenCalledWith("temp_99"),
    );
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy that fails",
      "steer:pending",
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    unsubscribe();
  });

  it("retries a non-session send failure through the queue", async () => {
    const markOptimisticFailed = vi.fn();
    const appendOptimistic = vi.fn().mockReturnValue("temp_fail_1");
    const discardOptimistic = vi.fn();
    mockStream([], { appendOptimistic, markOptimisticFailed, discardOptimistic });
    steerFleetActionMock
      .mockResolvedValueOnce({
        ok: false,
        error: "Provider unavailable",
        status: 503,
        errorCode: "UZ-AGT-503",
      })
      .mockResolvedValueOnce({ ok: true, data: { event_id: "evt_retry_ok" } });
    renderThread();
    await capturedOnNew.current!(appendMessage("retry this send"));
    await waitFor(() => expect(screen.getByRole("button", { name: "Retry" })).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(steerFleetActionMock).toHaveBeenCalledTimes(2));
    expect(markOptimisticFailed).toHaveBeenCalledTimes(1);
    // The stale failed row leaves the thread before the fresh optimistic
    // re-submit — otherwise every retry stacks a duplicate of the message.
    expect(discardOptimistic).toHaveBeenCalledWith("temp_fail_1");
  });

  it("marks the optimistic message failed when the action invocation throws", async () => {
    const refreshed = vi.fn();
    const unsubscribe = subscribeOnboardingRefresh(WS, refreshed);
    const appendOptimistic = vi.fn().mockReturnValue("temp_t");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, markOptimisticFailed });
    steerFleetActionMock.mockRejectedValueOnce(new Error("Server Component transport failed"));
    renderThread();
    await capturedOnNew.current!(appendMessage("offline send"));
    await waitFor(() =>
      expect(markOptimisticFailed).toHaveBeenCalledWith("temp_t"),
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
    expect(refreshed).not.toHaveBeenCalled();
    unsubscribe();
  });

  it("does not call the action when the submitted message text is empty", async () => {
    const appendOptimistic = vi.fn();
    mockStream([], { appendOptimistic });
    renderThread();
    await capturedOnNew.current!(appendMessage(""));
    expect(steerFleetActionMock).not.toHaveBeenCalled();
    expect(appendOptimistic).not.toHaveBeenCalled();
  });

  it("does not call the action when the append carries no text part", async () => {
    // The composer UI always emits a text part, but `onNew` may receive an
    // image-only append. `extractMessageText` must fall through to "" and the
    // empty-text guard must short-circuit before any optimistic write or remote call.
    const appendOptimistic = vi.fn().mockReturnValue("temp_img");
    mockStream([], { appendOptimistic });
    renderThread();
    expect(capturedOnNew.current).toBeTypeOf("function");
    await capturedOnNew.current!({
      content: [{ type: "image", image: "data:image/png;base64,xx" }],
    } as unknown as AppendMessage);
    expect(steerFleetActionMock).not.toHaveBeenCalled();
    expect(appendOptimistic).not.toHaveBeenCalled();
  });
});

describe("FleetThread — connection-state header", () => {
  it("renders the Reconnecting badge while connectionStatus=RECONNECTING", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    renderThread();
    expect(screen.getAllByText(/Reconnecting…/).length).toBeGreaterThan(0);
  });

  it("renders the Connecting badge while connectionStatus=CONNECTING", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    renderThread();
    expect(screen.getByText(/Connecting…/)).toBeTruthy();
  });
});

describe("FleetThread — robustness against malformed metadata", () => {
  it("does not throw when an event's custom.actor is a non-string", () => {
    // Simulate a frame whose convertEvent emits metadata.custom with a
    // non-string actor. The renderer must degrade to an empty actor label
    // rather than throw.
    const broken: FleetEvent = {
      id: "e_broken",
      role: "system",
      actor: "" as unknown as string,
      text: "config has non-string actor in custom",
      reply: "",
      outcome: OUTCOME.NO_REPLY,
      failureLabel: null,
      failureDetail: null,
      createdAt: new Date(Date.UTC(2026, 4, 15, 9, 0, 0)),
      status: "processed",
    };
    const customAnyConverter = (e: FleetEvent) => ({
      role: e.role,
      id: e.id,
      createdAt: e.createdAt,
      content: [{ type: "text" as const, text: e.text }],
      metadata: {
        custom: {
          actor: 42 as unknown as string,
          status: 99 as unknown as string,
          requestJson: { not: "a string" } as unknown as string,
        },
      },
    });
    useFleetEventStreamMock.mockReturnValue({
      events: [broken],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      markOptimisticFailed: vi.fn(),
      convertEvent: customAnyConverter,
    });
    expect(() => renderThread()).not.toThrow();
    expect(screen.getByText(/config has non-string actor/)).toBeTruthy();
  });

  it("degrades a non-string custom.status to neither queued nor failed", () => {
    // `readCustomStatus` reads metadata.custom.status; a frame whose converter
    // emits a non-string status (numeric here) must fall through to "" so the
    // user row is treated as settled — no optimistic "queued" / "failed" badge.
    const e = ev({ role: "user", actor: "steer:kishore@e2e.com", text: "non-string status" });
    useFleetEventStreamMock.mockReturnValue({
      events: [e],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      markOptimisticFailed: vi.fn(),
      convertEvent: (m: FleetEvent) => ({
        role: m.role,
        id: m.id,
        createdAt: m.createdAt,
        content: [{ type: "text" as const, text: m.text }],
        metadata: { custom: { actor: m.actor, status: 7 as unknown as string } },
      }),
    });
    const { container } = renderThread();
    const row = container.querySelector('[data-role="user"]');
    expect(row).toBeTruthy();
    expect(row?.getAttribute("data-optimistic")).toBeNull();
    expect(row?.getAttribute("data-failed")).toBeNull();
    expect(screen.queryByText(/^queued$/i)).toBeNull();
    expect(screen.queryByText(/^failed$/i)).toBeNull();
    expect(screen.getByText(/non-string status/)).toBeTruthy();
  });

  it("renders a user row whose converted content has no text part", () => {
    // `readText` iterates content for a `text` part; an image-only append
    // leaves it empty. The row must still carry its sender and its time —
    // the shape survives even when there is nothing to say.
    const e = ev({ role: "user", actor: "steer:user_3gkbg", text: "" });
    useFleetEventStreamMock.mockReturnValue({
      events: [e],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      markOptimisticFailed: vi.fn(),
      convertEvent: (m: FleetEvent) => ({
        role: m.role,
        id: m.id,
        createdAt: m.createdAt,
        content: [{ type: "image" as const, image: "data:image/png;base64,xx" }],
        metadata: { custom: { actor: m.actor, status: m.status } },
      }),
    });
    const { container } = renderThread();
    const row = container.querySelector('[data-role="user"]');
    expect(row).toBeTruthy();
    expect(row?.textContent).toContain("Operator");
    expect(row?.querySelector("time")).toBeTruthy();
  });

  it("viewport carries role=log, aria-live=polite, aria-label", () => {
    mockStream([ev({ role: "system", actor: "config_reload", text: "ok" })]);
    const { container } = renderThread();
    const viewport = container.querySelector('[role="log"]');
    expect(viewport).toBeTruthy();
    expect(viewport?.getAttribute("aria-live")).toBe("polite");
    expect(viewport?.getAttribute("aria-label")).toBe("Chat");
  });

  it("renders the backfill skeleton when CONNECTING with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeTruthy();
    expect(screen.queryByText(/Message this fleet or wait for its next trigger/i)).toBeNull();
  });

  it("renders the backfill skeleton when RECONNECTING with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeTruthy();
  });

  it("shows the idle empty-state hint (not skeleton) when LIVE with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeNull();
    expect(screen.getByText(/Message this fleet or wait for its next trigger/i)).toBeTruthy();
  });

  it("never renders the skeleton once any event is present", () => {
    mockStream(
      [ev({ role: "assistant", actor: "fleet", text: "first frame" })],
      { connectionStatus: CONNECTION_STATUS.CONNECTING },
    );
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeNull();
  });

  it("every rendered row carries the frame-enter fade-in classes", () => {
    mockStream([
      ev({ role: "user", actor: "steer:k@e2e.com", text: "u" }),
      ev({ role: "assistant", actor: "fleet", text: "a" }),
      ev({ role: "system", actor: "cron", text: "c" }),
      ev({
        role: "system",
        actor: "webhook:github",
        text: "wh",
        custom: { requestJson: "{}" },
      }),
    ]);
    const { container } = renderThread();
    const rows = container.querySelectorAll('[data-role]');
    expect(rows.length).toBeGreaterThanOrEqual(4);
    for (const r of rows) {
      const cls = r.className;
      expect(cls).toMatch(/animate-in/);
      expect(cls).toMatch(/fade-in-0/);
      expect(cls).toMatch(/duration-150/);
    }
  });

  it("renders the jump-to-latest scroll button", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "x" })]);
    renderThread();
    expect(screen.getByRole("button", { name: /jump to latest/i })).toBeTruthy();
  });

  it("keeps a fleet reply left aligned and its long body within the reading column", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "x" })]);
    const { container } = renderThread();
    const row = container.querySelector('[data-role="assistant"]') as HTMLElement;
    expect(row).toBeTruthy();
    expect((row.querySelector("[data-dashboard-row]") as HTMLElement).className).toMatch(/max-w-5xl/);
    const body = row.querySelector(".break-words");
    expect(body).toBeTruthy();
  });

  it("scrolls the conversation inside itself so the composer stays on screen", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "x" })]);
    const { container } = renderThread();
    const messageLog = container.querySelector('[role="log"]') as HTMLElement;
    const viewport = messageLog.parentElement?.parentElement as HTMLElement;
    const composer = container.querySelector('[aria-label="Chat composer"]');
    expect(messageLog).toBeTruthy();
    // The message list owns the overflow. Without this the card grows to the
    // height of its whole history and pushes the composer off the page.
    expect(viewport.className).toMatch(/overflow-y-auto/);
    expect(viewport.className).toMatch(/min-h-0/);
    expect(messageLog.contains(composer)).toBe(false);
    const card = container.querySelector('[aria-label="Fleet chat"]') as HTMLElement;
    expect(card.className).toMatch(/flex-col/);
    expect(card.className).toMatch(/min-h-0/);
  });

  it("renders a webhook row WITHOUT a payload block when requestJson is empty", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:slack",
        text: "Slack ping · no body",
        custom: { requestJson: "" },
      }),
    ]);
    renderThread();
    expect(screen.getByText(/Slack ping · no body/)).toBeTruthy();
    expect(screen.getByText("slack")).toBeTruthy();
    expect(screen.queryByText(/"action":/)).toBeNull();
    expect(screen.queryByText(/payload/i)).toBeNull();
  });

  it("does not lead an actionless repository title with an activity separator", () => {
    mockStream([
      ev({
        role: "system",
        actor: "webhook:github",
        text: "agentsfleet/agentsfleet#541 — Fix routing",
        custom: {
          requestJson:
            '{"repo":"agentsfleet/agentsfleet","number":541,"url":"https://github.com/agentsfleet/agentsfleet/pull/541"}',
        },
      }),
    ]);
    const { container } = renderThread();

    const tick = container.querySelector('[data-compact="true"]');
    expect(screen.getByText("Fix routing")).toBeTruthy();
    expect(tick?.textContent).not.toContain("· Fix routing");
  });
});
