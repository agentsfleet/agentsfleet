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

import type { ThreadMessageLike } from "@assistant-ui/react";

// ── Hoisted mocks ────────────────────────────────────────────────────────

const { steerZombieMock, useZombieEventStreamMock } = vi.hoisted(() => ({
  steerZombieMock: vi.fn(),
  useZombieEventStreamMock: vi.fn(),
}));

vi.mock("@/lib/api/zombies", () => ({
  steerZombie: steerZombieMock,
}));

vi.mock("@/components/domain/useZombieEventStream", async () => {
  const actual = await vi.importActual<
    typeof import("@/components/domain/useZombieEventStream")
  >("@/components/domain/useZombieEventStream");
  return {
    ...actual,
    useZombieEventStream: useZombieEventStreamMock,
  };
});

import { ZombieThread } from "@/components/domain/ZombieThread";
import { formatActorLabel } from "@/components/domain/zombieMessageRenderers";
import {
  CONNECTION_STATUS,
  type ZombieEvent,
} from "@/components/domain/useZombieEventStream";

// ── Fixture builders ─────────────────────────────────────────────────────

const WS = "ws_test";
const ZID = "zomb_test";
const TOKEN = "tok_test";

function ev(over: Partial<ZombieEvent> & { actor: string; role: ZombieEvent["role"] }): ZombieEvent {
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

function toThreadMessage(e: ZombieEvent): ThreadMessageLike {
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

function mockStream(
  events: ZombieEvent[],
  opts?: {
    isRunning?: boolean;
    connectionStatus?: (typeof CONNECTION_STATUS)[keyof typeof CONNECTION_STATUS];
  },
) {
  useZombieEventStreamMock.mockReturnValue({
    events,
    connectionStatus: opts?.connectionStatus ?? CONNECTION_STATUS.LIVE,
    isRunning: opts?.isRunning ?? false,
    appendOptimistic: vi.fn().mockReturnValue("temp_1"),
    reconcileOptimistic: vi.fn(),
    convertEvent: toThreadMessage,
    retryState: null,
  });
}

function renderThread(token: string | null = TOKEN) {
  return render(
    React.createElement(ZombieThread, {
      workspaceId: WS,
      zombieId: ZID,
      token,
    }),
  );
}

beforeEach(() => {
  steerZombieMock.mockReset();
  useZombieEventStreamMock.mockReset();
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

// ── ZombieThread integration ─────────────────────────────────────────────

describe("ZombieThread — empty state", () => {
  it("renders the waiting-for-activity hint when no events", () => {
    mockStream([]);
    renderThread();
    expect(
      screen.getByText(/waiting for activity/i),
    ).toBeTruthy();
    expect(screen.getByText(/0 events/i)).toBeTruthy();
  });
});

describe("ZombieThread — header chrome", () => {
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

describe("ZombieThread — role rendering", () => {
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
    // The steer glyph is rendered as plain text inside the user row.
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

describe("ZombieThread — composer disabled-while-running", () => {
  it("flips placeholder when isRunning toggles true", () => {
    mockStream(
      [ev({ role: "assistant", actor: "agent", text: "streaming…" })],
      { isRunning: true },
    );
    renderThread();
    expect(
      screen.getByPlaceholderText(/zombie is working/i),
    ).toBeTruthy();
  });

  it("uses the idle placeholder when not running", () => {
    mockStream([], { isRunning: false });
    renderThread();
    expect(screen.getByPlaceholderText(/steer this zombie/i)).toBeTruthy();
  });
});

describe("ZombieThread — steer submission", () => {
  it("calls steerZombie and reconciles the optimistic message on send", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_42");
    const reconcileOptimistic = vi.fn();
    useZombieEventStreamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic,
      reconcileOptimistic,
      convertEvent: toThreadMessage,
    retryState: null,
    });
    steerZombieMock.mockResolvedValueOnce({ event_id: "evt_real_42" });
    renderThread();
    const textarea = screen.getByPlaceholderText(/steer this zombie/i);
    fireEvent.change(textarea, { target: { value: "deploy the canary" } });
    fireEvent.submit(textarea.closest("form")!);
    await waitFor(() =>
      expect(steerZombieMock).toHaveBeenCalledWith(
        WS,
        ZID,
        "deploy the canary",
        TOKEN,
        expect.objectContaining({
          onRetry: expect.any(Function),
          onAttempt: expect.any(Function),
        }),
      ),
    );
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy the canary",
      "steer:pending",
    );
    expect(reconcileOptimistic).toHaveBeenCalledWith("temp_42", "evt_real_42");
  });

  it("keeps the optimistic message and does not throw when steerZombie rejects", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_99");
    const reconcileOptimistic = vi.fn();
    useZombieEventStreamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic,
      reconcileOptimistic,
      convertEvent: toThreadMessage,
    retryState: null,
    });
    steerZombieMock.mockRejectedValueOnce(new Error("HTTP 503 from steer"));
    renderThread();
    const textarea = screen.getByPlaceholderText(/steer this zombie/i);
    fireEvent.change(textarea, { target: { value: "deploy that fails" } });
    fireEvent.submit(textarea.closest("form")!);
    await waitFor(() => expect(steerZombieMock).toHaveBeenCalled());
    // Optimistic message was appended, but reconcile is NOT called: the
    // message stays in `optimistic` state for the user to retry.
    expect(appendOptimistic).toHaveBeenCalledWith(
      "deploy that fails",
      "steer:pending",
    );
    expect(reconcileOptimistic).not.toHaveBeenCalled();
  });

  it("does not call steerZombie when token is null", async () => {
    const appendOptimistic = vi.fn().mockReturnValue("temp_x");
    useZombieEventStreamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      isRunning: false,
      appendOptimistic,
      reconcileOptimistic: vi.fn(),
      convertEvent: toThreadMessage,
    retryState: null,
    });
    renderThread(null);
    const textarea = screen.getByPlaceholderText(/steer this zombie/i);
    fireEvent.change(textarea, { target: { value: "no token, no steer" } });
    fireEvent.submit(textarea.closest("form")!);
    // Wait a beat to give any errant async path a chance to fire.
    await new Promise((r) => setTimeout(r, 50));
    expect(steerZombieMock).not.toHaveBeenCalled();
    expect(appendOptimistic).not.toHaveBeenCalled();
  });
});

describe("ZombieThread — connection-state header", () => {
  it("renders the Reconnecting badge while connectionStatus=RECONNECTING", () => {
    useZombieEventStreamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.RECONNECTING,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      convertEvent: toThreadMessage,
    retryState: null,
    });
    renderThread();
    expect(screen.getByText(/Reconnecting…/)).toBeTruthy();
  });

  it("renders the Connecting badge while connectionStatus=CONNECTING", () => {
    useZombieEventStreamMock.mockReturnValue({
      events: [],
      connectionStatus: CONNECTION_STATUS.CONNECTING,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      convertEvent: toThreadMessage,
    retryState: null,
    });
    renderThread();
    expect(screen.getByText(/Connecting…/)).toBeTruthy();
  });
});

describe("ZombieThread — robustness against malformed metadata", () => {
  it("does not throw when an event's custom.actor is a non-string", () => {
    // Bypass the type system: simulate a frame whose convertEvent emits
    // metadata.custom with a non-string actor (e.g., the server ships a
    // number or null). The renderer must gracefully degrade to an empty
    // actor label rather than throw.
    const broken: ZombieEvent = {
      id: "e_broken",
      role: "system",
      actor: "" as unknown as string,
      text: "config has non-string actor in custom",
      createdAt: new Date(Date.UTC(2026, 4, 15, 9, 0, 0)),
      status: "processed",
    };
    const customAnyConverter = (e: ZombieEvent) => ({
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
    useZombieEventStreamMock.mockReturnValue({
      events: [broken],
      connectionStatus: CONNECTION_STATUS.LIVE,
      isRunning: false,
      appendOptimistic: vi.fn(),
      reconcileOptimistic: vi.fn(),
      convertEvent: customAnyConverter,
      retryState: null,
    });
    expect(() => renderThread()).not.toThrow();
    expect(screen.getByText(/config has non-string actor/)).toBeTruthy();
  });

  // ── Polish bundle (#16–#20) ─────────────────────────────────────────────

  it("#16 — viewport carries role=log, aria-live=polite, aria-label", () => {
    mockStream([ev({ role: "system", actor: "config_reload", text: "ok" })]);
    const { container } = renderThread();
    const viewport = container.querySelector('[role="log"]');
    expect(viewport).toBeTruthy();
    expect(viewport?.getAttribute("aria-live")).toBe("polite");
    expect(viewport?.getAttribute("aria-label")).toBe("Live activity");
  });

  it("#17 — renders the backfill skeleton when CONNECTING with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.CONNECTING });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeTruthy();
    // The idle empty-state hint is suppressed during connect.
    expect(screen.queryByText(/waiting for activity/i)).toBeNull();
  });

  it("#17 — renders the backfill skeleton when RECONNECTING with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeTruthy();
  });

  it("#17 — shows the idle empty-state hint (not skeleton) when LIVE with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeNull();
    expect(screen.getByText(/waiting for activity/i)).toBeTruthy();
  });

  it("#17 — never renders the skeleton once any event is present", () => {
    mockStream(
      [ev({ role: "assistant", actor: "agent", text: "first frame" })],
      { connectionStatus: CONNECTION_STATUS.CONNECTING },
    );
    const { container } = renderThread();
    expect(container.querySelector('[data-testid="backfill-skeleton"]')).toBeNull();
  });

  it("#18 — every rendered row carries the frame-enter fade-in classes", () => {
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

  it("#19 — renders the jump-to-latest scroll button", () => {
    mockStream([ev({ role: "assistant", actor: "agent", text: "x" })]);
    renderThread();
    expect(screen.getByRole("button", { name: /jump to latest/i })).toBeTruthy();
  });

  it("#20 — message rows apply responsive grid modifiers + actor-rail var", () => {
    mockStream([ev({ role: "assistant", actor: "agent", text: "x" })]);
    const { container } = renderThread();
    const row = container.querySelector('[data-role="assistant"]') as HTMLElement;
    expect(row).toBeTruthy();
    // Single column at <md, project-token grid template at md+.
    expect(row.className).toMatch(/grid-cols-1/);
    expect(row.className).toMatch(/md:grid-cols-\[var\(--actor-rail-w\)_1fr\]/);
    // The actor-rail CSS variable is injected inline so the grid template
    // resolves without per-row repetition of the literal width.
    expect(row.style.getPropertyValue("--actor-rail-w")).toBe("72px");
  });

  it("#20 — composer stacks vertically at <sm, row-aligned at sm+", () => {
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
    // Source tag still renders.
    expect(screen.getByText("slack")).toBeTruthy();
    // The collapsible summary line exists, but the <pre> payload is omitted
    // when requestJson is empty. Asserting absence by role:
    expect(screen.queryByText(/"action":/)).toBeNull();
  });
});
