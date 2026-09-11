import { ev, mockStream, renderThread, useFleetEventStreamMock } from "./harness";
import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import { OUTCOME } from "@/lib/events/event-summary";
import { FleetThread } from "@/components/domain/FleetThread";
import { CONNECTION_STATUS, type FleetEvent } from "@/components/domain/useFleetEventStream";

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
      outcome: OUTCOME.COMPLETED,
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
    const e = ev({
      role: "user",
      actor: "steer:kishore@e2e.com",
      text: "non-string status",
    });
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
        metadata: {
          custom: { actor: m.actor, status: 7 as unknown as string },
        },
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
        content: [
          { type: "image" as const, image: "data:image/png;base64,xx" },
        ],
        metadata: { custom: { actor: m.actor, status: m.status } },
      }),
    });
    const { container } = renderThread();
    const row = container.querySelector('[data-role="user"]');
    expect(row).toBeTruthy();
    expect(row?.textContent).toBe("Operator: ");
    expect(row?.querySelector(".sr-only")?.textContent).toBe("Operator: ");
    expect(row?.querySelector("time")).toBeNull();
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
    expect(
      container.querySelector('[data-testid="backfill-skeleton"]'),
    ).toBeTruthy();
    expect(
      screen.queryByText(/Message this fleet or wait for its next trigger/i),
    ).toBeNull();
  });

  it("renders the backfill skeleton when RECONNECTING with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.RECONNECTING });
    const { container } = renderThread();
    expect(
      container.querySelector('[data-testid="backfill-skeleton"]'),
    ).toBeTruthy();
  });

  it("shows the idle empty-state hint (not skeleton) when LIVE with no events", () => {
    mockStream([], { connectionStatus: CONNECTION_STATUS.LIVE });
    const { container } = renderThread();
    expect(
      container.querySelector('[data-testid="backfill-skeleton"]'),
    ).toBeNull();
    expect(
      screen.getByText(/Message this fleet or wait for its next trigger/i),
    ).toBeTruthy();
  });

  it("never renders the skeleton once any event is present", () => {
    mockStream(
      [ev({ role: "assistant", actor: "fleet", text: "first frame" })],
      {
        connectionStatus: CONNECTION_STATUS.CONNECTING,
      },
    );
    const { container } = renderThread();
    expect(
      container.querySelector('[data-testid="backfill-skeleton"]'),
    ).toBeNull();
  });

  it("every rendered row carries the operational 80ms fade-in classes", () => {
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
    const rows = container.querySelectorAll("[data-role]");
    expect(rows.length).toBeGreaterThanOrEqual(4);
    for (const r of rows) {
      const cls = r.className;
      expect(cls).toMatch(/animate-in/);
      expect(cls).toMatch(/fade-in-0/);
      expect(cls).toMatch(/duration-stream/);
      expect(cls).not.toMatch(/slide-in/);
    }
  });

  it("renders the jump-to-latest scroll button", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "x" })]);
    renderThread();
    expect(
      screen.getByRole("button", { name: /jump to latest/i }),
    ).toBeTruthy();
  });

  it("keeps a fleet reply left aligned and its long body within the reading column", () => {
    mockStream([ev({ role: "assistant", actor: "fleet", text: "x" })]);
    const { container } = renderThread();
    const row = container.querySelector(
      '[data-role="assistant"]',
    ) as HTMLElement;
    expect(row).toBeTruthy();
    expect(row.querySelector(".flex-row-reverse")).toBeNull();
    expect(row.querySelector(".max-w-prose")).toBeTruthy();
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
    const card = container.querySelector(
      '[aria-label="Fleet chat"]',
    ) as HTMLElement;
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
