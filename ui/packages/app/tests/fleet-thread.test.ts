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
import { formatActorLabel } from "@/components/domain/fleetMessageRenderers";
import { subscribeOnboardingRefresh } from "@/lib/onboarding-refresh";
import type { EventRow } from "@/lib/api/events";
import {
  CONNECTION_STATUS,
  type FleetEvent,
} from "@/components/domain/useFleetEventStream";

// ── Fixture builders ─────────────────────────────────────────────────────

const WS = "ws_test";
const ZID = "zomb_test";

function ev(over: Partial<FleetEvent> & { actor: string; role: FleetEvent["role"] }): FleetEvent {
  return {
    id: over.id ?? `e_${Math.random().toString(36).slice(2, 8)}`,
    role: over.role,
    actor: over.actor,
    text: over.text ?? "",
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
        reason: e.custom?.reason,
        status: e.status,
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

function renderThread() {
  return renderThreadWithInitial([]);
}

function renderThreadWithInitial(initial: EventRow[]) {
  return render(
    React.createElement(FleetThread, {
      workspaceId: WS,
      fleetId: ZID,
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
  capturedOnNew.current = null;
  capturedRetry.current = null;
});

afterEach(() => cleanup());

// ── formatActorLabel unit ────────────────────────────────────────────────

describe("formatActorLabel", () => {
  it("returns local-part first segment lowercase for steer actors", () => {
    expect(formatActorLabel("steer:Kishore.Kumar@e2enetworks.com")).toBe(
      "kishore",
    );
  });
  it("returns 'fleet' verbatim", () => {
    expect(formatActorLabel("fleet")).toBe("fleet");
  });
  it("formats webhook actors with separator", () => {
    expect(formatActorLabel("webhook:github")).toBe("webhook · github");
  });
  it("lowercases unknown actors", () => {
    expect(formatActorLabel("Cron")).toBe("cron");
  });
});

// ── FleetThread integration ─────────────────────────────────────────────

describe("FleetThread — empty state", () => {
  it("renders the waiting-for-activity hint when no events", () => {
    mockStream([]);
    renderThread();
    expect(screen.getByText(/Message this fleet or wait for its next trigger/i)).toBeTruthy();
    expect(screen.getByText(/0 events/i)).toBeTruthy();
  });
});

describe("FleetThread — header chrome", () => {
  it("shows panel title, frame count, and Live badge while connected", () => {
    mockStream([
      ev({ role: "system", actor: "config_reload", text: "Reloaded" }),
    ]);
    renderThread();
    expect(screen.getByText(/^Chat$/)).toBeTruthy();
    expect(screen.getByText(/1 events/)).toBeTruthy();
    expect(screen.getByText(/^Live$/)).toBeTruthy();
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
      React.createElement(FleetThread, { workspaceId: WS, fleetId: ZID, initial: [] }),
    );

    await waitFor(() => expect(routerRefreshMock).toHaveBeenCalledTimes(1));
    view.rerender(
      React.createElement(FleetThread, { workspaceId: WS, fleetId: ZID, initial: [] }),
    );
    expect(routerRefreshMock).toHaveBeenCalledTimes(1);
  });
});

describe("FleetThread — role rendering", () => {
  it("renders a user steer with the › pulse prefix", () => {
    mockStream([
      ev({
        role: "user",
        actor: "steer:kishore@e2e.com",
        text: "morning health check",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/morning health check/)).toBeTruthy();
    const glyphs = screen.getAllByText("›");
    expect(glyphs.length).toBeGreaterThan(0);
  });

  it("renders an assistant message in sans body text", () => {
    mockStream([
      ev({ role: "assistant", actor: "fleet", text: "snapshot taken." }),
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
    expect(screen.getByText("cron")).toBeTruthy();
    expect(screen.getByText(/tick/)).toBeTruthy();
  });

  it("renders a continuation system row with its chip label", () => {
    mockStream([ev({ role: "system", actor: "continuation", text: "resumed after gate" })]);
    renderThread();
    expect(screen.getByText("continuation")).toBeTruthy();
    expect(screen.getByText(/resumed after gate/)).toBeTruthy();
  });

  it("renders a gate_blocked system row with its chip label", () => {
    mockStream([ev({ role: "system", actor: "gate_blocked", text: "blocked on approval" })]);
    renderThread();
    expect(screen.getByText("gate_blocked")).toBeTruthy();
    expect(screen.getByText(/blocked on approval/)).toBeTruthy();
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
    expect(screen.getByText("github")).toBeTruthy();
    expect(screen.getByText(/workflow_run · main · success/)).toBeTruthy();
    expect(screen.getByText(/"action":"completed"/)).toBeTruthy();
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
    expect(screen.getByText(/^queued$/i)).toBeTruthy();
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
    expect(screen.getByText(/^failed$/i)).toBeTruthy();
    // The optimistic "queued" badge must not also render for a failed row.
    expect(screen.queryByText(/^queued$/i)).toBeNull();
  });

  it("renders a fleet_error as a destructive meta-row", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "fleet",
        text: "Provider returned 429; retry budget exhausted",
        status: "fleet_error",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/fleet_error/)).toBeTruthy();
    expect(screen.getByText(/Provider returned 429/)).toBeTruthy();
  });
});

describe("FleetThread — fluid composer", () => {
  it("keeps the message field available while running", () => {
    mockStream(
      [ev({ role: "assistant", actor: "fleet", text: "streaming…" })],
      { isRunning: true },
    );
    renderThread();
    const input = screen.getByPlaceholderText(/message this fleet/i) as HTMLTextAreaElement;
    expect(input.disabled).toBe(false);
    expect(screen.getByText(/new messages will queue/i)).toBeTruthy();
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
    mockStream([], { markOptimisticFailed });
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
    // empty-text guard must short-circuit before any optimistic write/RPC.
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
    // `readText` iterates content for a `text` part; an image-only user
    // append leaves it empty, so the row paints the steer glyph with no body.
    const e = ev({ role: "user", actor: "steer:kishore@e2e.com", text: "" });
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
    // Glyph renders; body text is empty because readText found no text part.
    expect(row?.textContent).toContain("›");
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

  it("message rows apply responsive grid modifiers + actor-rail var", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "x" })]);
    const { container } = renderThread();
    const row = container.querySelector('[data-role="assistant"]') as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.className).toMatch(/grid-cols-1/);
    expect(row.className).toMatch(/md:grid-cols-\[var\(--actor-rail-w\)_1fr\]/);
    expect(row.style.getPropertyValue("--actor-rail-w")).toBe("72px");
  });

  it("composer stacks vertically at <sm, row-aligned at sm+", () => {
    mockStream([]);
    const { container } = renderThread();
    const composerInner = container.querySelector('[aria-label="Chat composer"] > div:last-child');
    expect(composerInner).toBeTruthy();
    const cls = composerInner!.className;
    expect(cls).toMatch(/flex-col/);
    expect(cls).toMatch(/sm:flex-row/);
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
  });
});
