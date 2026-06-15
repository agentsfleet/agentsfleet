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
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import type { AppendMessage, ThreadMessageLike } from "@assistant-ui/react";

// ── Hoisted mocks ────────────────────────────────────────────────────────

const { steerAgentActionMock, useAgentEventStreamMock, capturedOnNew } =
  vi.hoisted(() => ({
    steerAgentActionMock: vi.fn(),
    useAgentEventStreamMock: vi.fn(),
    // Capture the `onNew` callback wired into the external-store runtime so a
    // test can drive it with content the composer UI never emits (e.g. an
    // image-only append) to reach `extractMessageText`'s no-text-part path.
    capturedOnNew: { current: null as ((msg: AppendMessage) => Promise<void>) | null },
  }));

vi.mock("@/app/(dashboard)/agents/actions", () => ({
  steerAgentAction: steerAgentActionMock,
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

vi.mock("@/components/domain/useAgentEventStream", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/domain/useAgentEventStream")
  >("@/components/domain/useAgentEventStream");
  return {
    ...actual,
    useAgentEventStream: useAgentEventStreamMock,
  };
});

import { AgentThread } from "@/components/domain/AgentThread";
import { formatActorLabel } from "@/components/domain/agentMessageRenderers";
import {
  CONNECTION_STATUS,
  type AgentEvent,
} from "@/components/domain/useAgentEventStream";

// ── Fixture builders ─────────────────────────────────────────────────────

const WS = "ws_test";
const ZID = "zomb_test";

function ev(over: Partial<AgentEvent> & { actor: string; role: AgentEvent["role"] }): AgentEvent {
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

function toThreadMessage(e: AgentEvent): ThreadMessageLike {
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
  events?: AgentEvent[];
  isRunning?: boolean;
  connectionStatus?: (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];
  appendOptimistic?: ReturnType<typeof vi.fn>;
  reconcileOptimistic?: ReturnType<typeof vi.fn>;
  markOptimisticFailed?: ReturnType<typeof vi.fn>;
};

function mockStream(events: AgentEvent[], opts?: Omit<StreamMockOverrides, "events">) {
  useAgentEventStreamMock.mockReturnValue({
    events,
    connectionStatus: opts?.connectionStatus ?? CONNECTION_STATUS.LIVE,
    isRunning: opts?.isRunning ?? false,
    appendOptimistic: opts?.appendOptimistic ?? vi.fn().mockReturnValue("temp_1"),
    reconcileOptimistic: opts?.reconcileOptimistic ?? vi.fn(),
    markOptimisticFailed: opts?.markOptimisticFailed ?? vi.fn(),
    convertEvent: toThreadMessage,
  });
}

function renderThread() {
  return render(
    React.createElement(AgentThread, {
      workspaceId: WS,
      agentId: ZID,
      initial: [],
    }),
  );
}

beforeEach(() => {
  steerAgentActionMock.mockReset();
  useAgentEventStreamMock.mockReset();
  capturedOnNew.current = null;
});

afterEach(() => cleanup());

// ── formatActorLabel unit ────────────────────────────────────────────────

describe("formatActorLabel", () => {
  it("returns local-part first segment lowercase for steer actors", () => {
    expect(formatActorLabel("steer:Kishore.Kumar@e2enetworks.com")).toBe(
      "kishore",
    );
  });
  it("returns 'agent' verbatim", () => {
    expect(formatActorLabel("agent")).toBe("agent");
  });
  it("formats webhook actors with separator", () => {
    expect(formatActorLabel("webhook:github")).toBe("webhook · github");
  });
  it("lowercases unknown actors", () => {
    expect(formatActorLabel("Cron")).toBe("cron");
  });
});

// ── AgentThread integration ─────────────────────────────────────────────

describe("AgentThread — empty state", () => {
  it("renders the waiting-for-activity hint when no events", () => {
    mockStream([]);
    renderThread();
    expect(screen.getByText(/waiting for activity/i)).toBeTruthy();
    expect(screen.getByText(/0 events/i)).toBeTruthy();
  });
});

describe("AgentThread — header chrome", () => {
  it("shows panel title, frame count, and Live badge while connected", () => {
    mockStream([
      ev({ role: "system", actor: "config_reload", text: "Reloaded" }),
    ]);
    renderThread();
    expect(screen.getByText(/^Live activity$/)).toBeTruthy();
    expect(screen.getByText(/1 events/)).toBeTruthy();
    expect(screen.getByText(/^Live$/)).toBeTruthy();
  });
});

describe("AgentThread — role rendering", () => {
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
      ev({ role: "assistant", actor: "agent", text: "snapshot taken." }),
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

  it("renders an agent_error as a destructive meta-row", () => {
    mockStream([
      ev({
        role: "assistant",
        actor: "agent",
        text: "Provider returned 429; retry budget exhausted",
        status: "agent_error",
      }),
    ]);
    renderThread();
    expect(screen.getByText(/agent_error/)).toBeTruthy();
    expect(screen.getByText(/Provider returned 429/)).toBeTruthy();
  });
});

describe("AgentThread — composer disabled-while-running", () => {
  it("flips placeholder when isRunning toggles true", () => {
    mockStream(
      [ev({ role: "assistant", actor: "agent", text: "streaming…" })],
      { isRunning: true },
    );
    renderThread();
    expect(screen.getByPlaceholderText(/agent is working/i)).toBeTruthy();
  });

  it("uses the idle placeholder when not running", () => {
    mockStream([], { isRunning: false });
    renderThread();
    expect(screen.getByPlaceholderText(/steer this agent/i)).toBeTruthy();
  });
});

describe("AgentThread — steer submission", () => {
  it("calls steerAgentAction and reconciles the optimistic message on ok", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_42");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, markOptimisticFailed });
    steerAgentActionMock.mockResolvedValueOnce({
      ok: true,
      data: { event_id: "evt_real_42" },
    });
    renderThread();
    const textarea = screen.getByPlaceholderText(/steer this agent/i);
    fireEvent.change(textarea, { target: { value: "deploy the canary" } });
    fireEvent.submit(textarea.closest("form")!);
    await waitFor(() =>
      expect(steerAgentActionMock).toHaveBeenCalledWith(
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
  });

  it("marks the optimistic message failed when the action returns ok:false", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_99");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, markOptimisticFailed });
    steerAgentActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
      errorCode: "UZ-AUTH-401",
    });
    renderThread();
    const textarea = screen.getByPlaceholderText(/steer this agent/i);
    fireEvent.change(textarea, { target: { value: "deploy that fails" } });
    fireEvent.submit(textarea.closest("form")!);
    await waitFor(() =>
      expect(markOptimisticFailed).toHaveBeenCalledWith("temp_99"),
    );
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy that fails",
      "steer:pending",
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
  });

  it("marks the optimistic message failed when the action invocation throws", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_t");
    const reconcileOptimistic = vi.fn();
    const markOptimisticFailed = vi.fn();
    mockStream([], { appendOptimistic, reconcileOptimistic, markOptimisticFailed });
    steerAgentActionMock.mockRejectedValueOnce(new Error("RSC transport failed"));
    renderThread();
    const textarea = screen.getByPlaceholderText(/steer this agent/i);
    fireEvent.change(textarea, { target: { value: "offline send" } });
    fireEvent.submit(textarea.closest("form")!);
    await waitFor(() =>
      expect(markOptimisticFailed).toHaveBeenCalledWith("temp_t"),
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
  });

  it("does not call the action when the submitted message text is empty", async () => {
    const appendOptimistic = vi.fn();
    mockStream([], { appendOptimistic });
    renderThread();
    const textarea = screen.getByPlaceholderText(/steer this agent/i);
    fireEvent.submit(textarea.closest("form")!);
    await new Promise((r) => setTimeout(r, 30));
    expect(steerAgentActionMock).not.toHaveBeenCalled();
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
    expect(steerAgentActionMock).not.toHaveBeenCalled();
    expect(appendOptimistic).not.toHaveBeenCalled();
  });
});

describe("AgentThread — connection-state header", () => {
  it("renders the Reconnecting badge while connectionStatus=RECONNECTING", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    renderThread();
    expect(screen.getByText(/Reconnecting…/)).toBeTruthy();
  });

  it("renders the Connecting badge while connectionStatus=CONNECTING", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    renderThread();
    expect(screen.getByText(/Connecting…/)).toBeTruthy();
  });
});

describe("AgentThread — robustness against malformed metadata", () => {
  it("does not throw when an event's custom.actor is a non-string", () => {
    // Simulate a frame whose convertEvent emits metadata.custom with a
    // non-string actor. The renderer must degrade to an empty actor label
    // rather than throw.
    const broken: AgentEvent = {
      id: "e_broken",
      role: "system",
      actor: "" as unknown as string,
      text: "config has non-string actor in custom",
      createdAt: new Date(Date.UTC(2026, 4, 15, 9, 0, 0)),
      status: "processed",
    };
    const customAnyConverter = (e: AgentEvent) => ({
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
    useAgentEventStreamMock.mockReturnValue({
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
    useAgentEventStreamMock.mockReturnValue({
      events: [e],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      markOptimisticFailed: vi.fn(),
      convertEvent: (m: AgentEvent) => ({
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
    useAgentEventStreamMock.mockReturnValue({
      events: [e],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      markOptimisticFailed: vi.fn(),
      convertEvent: (m: AgentEvent) => ({
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
    expect(viewport?.getAttribute("aria-label")).toBe("Live activity");
  });

  it("renders the backfill skeleton when CONNECTING with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeTruthy();
    expect(screen.queryByText(/waiting for activity/i)).toBeNull();
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
    expect(screen.getByText(/waiting for activity/i)).toBeTruthy();
  });

  it("never renders the skeleton once any event is present", () => {
    mockStream(
      [ev({ role: "assistant", actor: "agent", text: "first frame" })],
      { connectionStatus: CONNECTION_STATUS.CONNECTING },
    );
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeNull();
  });

  it("every rendered row carries the frame-enter fade-in classes", () => {
    mockStream([
      ev({ role: "user", actor: "steer:k@e2e.com", text: "u" }),
      ev({ role: "assistant", actor: "agent", text: "a" }),
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
    mockStream([ev({ role: "assistant", actor: "agent", text: "x" })]);
    renderThread();
    expect(screen.getByRole("button", { name: /jump to latest/i })).toBeTruthy();
  });

  it("message rows apply responsive grid modifiers + actor-rail var", () => {
    mockStream([ev({ role: "assistant", actor: "agent", text: "x" })]);
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
    const composerInner = container.querySelector('[aria-label="Steer composer"] > div');
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
