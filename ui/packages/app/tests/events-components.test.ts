import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// ── Shared mocks ───────────────────────────────────────────────────────────

const { getTokenFn, listZombieEventsMock, listWorkspaceEventsMock } = vi.hoisted(() => ({
  getTokenFn: vi.fn(),
  listZombieEventsMock: vi.fn(),
  listWorkspaceEventsMock: vi.fn(),
}));

vi.mock("@clerk/nextjs", () => ({
  useAuth: () => ({ getToken: getTokenFn }),
  useUser: () => ({ isLoaded: true, isSignedIn: true, user: null }),
  ClerkProvider: ({ children }: { children: React.ReactNode }) =>
    React.createElement(React.Fragment, null, children),
  UserButton: () => React.createElement("div"),
  SignIn: () => React.createElement("div"),
  SignUp: () => React.createElement("div"),
}));

vi.mock("@/lib/api/events", async () => {
  const actual = await vi.importActual<typeof import("@/lib/api/events")>(
    "@/lib/api/events",
  );
  return {
    ...actual,
    listZombieEvents: listZombieEventsMock,
    listWorkspaceEvents: listWorkspaceEventsMock,
  };
});

beforeEach(() => {
  vi.clearAllMocks();
  getTokenFn.mockResolvedValue("token_abc");
});

afterEach(() => cleanup());

// ── EventsList ─────────────────────────────────────────────────────────────

import { EventsList } from "../components/domain/EventsList";
import type { EventRow, EventsPage } from "@/lib/api/events";
import { TooltipProvider } from "@usezombie/design-system";

function row(over: Partial<EventRow> = {}): EventRow {
  const now = Date.UTC(2026, 3, 28, 10, 30, 0);
  return {
    event_id: "evt_1",
    zombie_id: "zomb_1234567890ab",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "hello world",
    tokens: 1,
    wall_ms: 10,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

function renderList(
  initial: EventsPage,
  scope:
    | { kind: "zombie"; workspaceId: string; zombieId: string }
    | { kind: "workspace"; workspaceId: string } = {
    kind: "zombie",
    workspaceId: "ws_1",
    zombieId: "zomb_1",
  },
  extra: { emptyTitle?: string; emptyDescription?: string } = {},
) {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(EventsList, { scope, initial, ...extra }),
    ),
  );
}

describe("EventsList", () => {
  it("renders default empty state when no items", () => {
    renderList({ items: [], next_cursor: null });
    expect(screen.getByText(/No events yet/i)).toBeTruthy();
    expect(screen.getByText(/Operator steers, webhooks/i)).toBeTruthy();
  });

  it("respects custom empty copy", () => {
    renderList(
      { items: [], next_cursor: null },
      { kind: "zombie", workspaceId: "ws_1", zombieId: "zomb_1" },
      { emptyTitle: "Nothing Here", emptyDescription: "yet" },
    );
    expect(screen.getByText(/Nothing Here/)).toBeTruthy();
    expect(screen.getByText(/^yet$/)).toBeTruthy();
  });

  it("renders one row per event with status badge, actor, and preview", () => {
    renderList({
      items: [
        row({ event_id: "a", status: "processed", response_text: "first event" }),
        row({ event_id: "b", status: "agent_error", response_text: null, failure_label: "boom" }),
        row({ event_id: "c", status: "gate_blocked", response_text: null }),
        row({ event_id: "d", status: "received", response_text: "rec" }),
        row({ event_id: "e", status: "weird-unknown", response_text: "fallback variant" }),
      ],
      next_cursor: null,
    });
    expect(screen.getByText("first event")).toBeTruthy();
    // failure_label fallback for null response_text
    expect(screen.getByText(/Reason: boom/)).toBeTruthy();
    // weird status falls through to default badge variant (still rendered)
    expect(screen.getByText("weird-unknown")).toBeTruthy();
    // listitems should equal items.length
    expect(screen.getAllByRole("listitem").length).toBe(5);
  });

  it("collapses whitespace and truncates long preview text to 160 chars", () => {
    const long = "x".repeat(300);
    renderList({
      items: [row({ event_id: "z", response_text: `  multi  \n  line   ${long}` })],
      next_cursor: null,
    });
    const article = screen.getByLabelText(/Event z by /);
    const para = article.querySelector("p");
    expect(para).toBeTruthy();
    const txt = para!.textContent ?? "";
    expect(txt.length).toBeLessThanOrEqual(161); // 157 + "…"
    expect(txt.endsWith("…")).toBe(true);
    expect(txt).not.toMatch(/\s\s/);
  });

  it("renders short zombie id only in workspace scope, full label in aria", () => {
    renderList(
      {
        items: [row({ zombie_id: "zomb_abcdefghijkl" })],
        next_cursor: null,
      },
      { kind: "workspace", workspaceId: "ws_1" },
    );
    // shortId: first 4 + … + last 4
    expect(screen.getByText(/zomb…ijkl/)).toBeTruthy();
  });

  it("does not render zombie id suffix in zombie scope", () => {
    renderList({
      items: [row({ zombie_id: "zomb_abcdefghijkl" })],
      next_cursor: null,
    });
    expect(screen.queryByText(/zomb…ijkl/)).toBeNull();
  });

  it("shortId returns the id verbatim when length <= 12", () => {
    renderList(
      {
        items: [row({ zombie_id: "abc12345" })],
        next_cursor: null,
      },
      { kind: "workspace", workspaceId: "ws_1" },
    );
    expect(screen.getByText(/· abc12345/)).toBeTruthy();
  });

  it("renders <time> with ISO when created_at is valid; omits when invalid", () => {
    const { container } = renderList({
      items: [
        row({ event_id: "ok", created_at: Date.UTC(2026, 0, 2, 3, 4, 0) }),
        row({ event_id: "bad", created_at: Number.NaN as unknown as number }),
      ],
      next_cursor: null,
    });
    const times = container.querySelectorAll("time");
    expect(times.length).toBe(1);
    expect(times[0]!.getAttribute("datetime")).toMatch(/^2026-01-02T/);
    // Locale-aware HH:MM (Intl.DateTimeFormat) — accept either 24h
    // ("13:04") or 12h with AM/PM ("01:04 pm"). The exact format depends
    // on the test runner's resolved locale.
    expect(times[0]!.textContent).toMatch(/^\d{2}:\d{2}(\s?[ap]m)?$/i);
  });

  it("loadMore (zombie scope) appends items and updates cursor", async () => {
    listZombieEventsMock.mockResolvedValueOnce({
      items: [row({ event_id: "p2", response_text: "page two" })],
      next_cursor: "cur_2",
    });
    renderList({
      items: [row({ event_id: "p1", response_text: "page one" })],
      next_cursor: "cur_1",
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() => expect(listZombieEventsMock).toHaveBeenCalled());
    expect(listZombieEventsMock).toHaveBeenCalledWith("ws_1", "zomb_1", "token_abc", {
      cursor: "cur_1",
    });
    await waitFor(() => expect(screen.getByText("page two")).toBeTruthy());
  });

  it("loadMore (workspace scope) calls workspace API", async () => {
    listWorkspaceEventsMock.mockResolvedValueOnce({
      items: [row({ event_id: "wsp", response_text: "ws page" })],
      next_cursor: null,
    });
    renderList(
      {
        items: [row({ event_id: "wsp0", response_text: "ws first" })],
        next_cursor: "cur_a",
      },
      { kind: "workspace", workspaceId: "ws_42" },
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() => expect(listWorkspaceEventsMock).toHaveBeenCalled());
    expect(listWorkspaceEventsMock).toHaveBeenCalledWith("ws_42", "token_abc", {
      cursor: "cur_a",
    });
  });

  it("loadMore surfaces 'Not authenticated' when token is null", async () => {
    getTokenFn.mockResolvedValueOnce(null);
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
    expect(listZombieEventsMock).not.toHaveBeenCalled();
  });

  it("loadMore surfaces error message when API rejects", async () => {
    listZombieEventsMock.mockRejectedValueOnce(new Error("backend down"));
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/backend down/),
    );
  });

  it("loadMore falls back to default message when error has no message", async () => {
    listZombieEventsMock.mockRejectedValueOnce({} as unknown as Error);
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Failed to load more events/),
    );
  });
});

// ── LiveEventsPanel ────────────────────────────────────────────────────────

import { LiveEventsPanel } from "../components/domain/LiveEventsPanel";
import { FRAME_KIND, type LiveFrame } from "@/lib/api/events";

type EsHandlers = {
  onopen: ((this: EventSource, ev: Event) => unknown) | null;
  onmessage: ((this: EventSource, ev: MessageEvent) => unknown) | null;
  onerror: ((this: EventSource, ev: Event) => unknown) | null;
};

class FakeEventSource implements EsHandlers {
  static instances: FakeEventSource[] = [];
  url: string;
  onopen: EsHandlers["onopen"] = null;
  onmessage: EsHandlers["onmessage"] = null;
  onerror: EsHandlers["onerror"] = null;
  closed = false;
  constructor(url: string) {
    this.url = url;
    FakeEventSource.instances.push(this);
  }
  close() {
    this.closed = true;
  }
  emit(frame: LiveFrame | string) {
    const data = typeof frame === "string" ? frame : JSON.stringify(frame);
    this.onmessage?.call(this as unknown as EventSource, { data } as MessageEvent);
  }
  open() {
    this.onopen?.call(this as unknown as EventSource, {} as Event);
  }
  fail() {
    this.onerror?.call(this as unknown as EventSource, {} as Event);
  }
}

describe("LiveEventsPanel", () => {
  beforeEach(() => {
    FakeEventSource.instances = [];
    (globalThis as unknown as { EventSource: unknown }).EventSource = FakeEventSource;
  });

  afterEach(() => {
    delete (globalThis as { EventSource?: unknown }).EventSource;
  });

  function mount(props: Partial<React.ComponentProps<typeof LiveEventsPanel>> = {}) {
    return render(
      React.createElement(LiveEventsPanel, {
        workspaceId: "ws_1",
        zombieId: "zomb_1",
        ...props,
      }),
    );
  }

  it("opens an EventSource against the same-origin stream URL on mount", () => {
    mount();
    expect(FakeEventSource.instances.length).toBe(1);
    expect(FakeEventSource.instances[0]!.url).toBe(
      "/backend/v1/workspaces/ws_1/zombies/zomb_1/events/stream",
    );
  });

  it("starts in connecting state, transitions to live on onopen", async () => {
    mount();
    expect(screen.getByText(/Connecting…/)).toBeTruthy();
    act(() => FakeEventSource.instances[0]!.open());
    await waitFor(() => expect(screen.getByText(/^Live$/)).toBeTruthy());
  });

  it("renders waiting message when no frames have arrived", () => {
    mount();
    expect(screen.getByText(/Waiting for activity/)).toBeTruthy();
  });

  it("appends each well-formed frame and renders the kind summary", async () => {
    mount();
    const es = FakeEventSource.instances[0]!;
    act(() => es.open());
    const frames: LiveFrame[] = [
      { kind: FRAME_KIND.EVENT_RECEIVED, event_id: "evt_1", actor: "alice" },
      { kind: FRAME_KIND.TOOL_CALL_STARTED, event_id: "evt_1", name: "fetch", args_redacted: {} },
      { kind: FRAME_KIND.TOOL_CALL_PROGRESS, event_id: "evt_1", name: "fetch", elapsed_ms: 250 },
      { kind: FRAME_KIND.CHUNK, event_id: "evt_1", text: "hello chunk" },
      { kind: FRAME_KIND.TOOL_CALL_COMPLETED, event_id: "evt_1", name: "fetch", ms: 999 },
      { kind: FRAME_KIND.EVENT_COMPLETE, event_id: "evt_1", status: "processed" },
    ];
    for (const f of frames) act(() => es.emit(f));
    await waitFor(() => expect(screen.getAllByRole("listitem").length).toBe(6));
    expect(screen.getByText(/alice → evt_1/)).toBeTruthy();
    expect(screen.getAllByText(/^fetch$/).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/fetch · 250ms/)).toBeTruthy();
    expect(screen.getByText(/hello chunk/)).toBeTruthy();
    expect(screen.getByText(/fetch · 999ms/)).toBeTruthy();
    expect(screen.getByText(/evt_1 · processed/)).toBeTruthy();
  });

  it("drops malformed frames: invalid JSON, non-object, missing kind", async () => {
    mount();
    const es = FakeEventSource.instances[0]!;
    act(() => es.open());
    act(() => es.emit("{not json"));
    act(() => es.emit("null"));
    act(() => es.emit("123"));
    act(() => es.emit(JSON.stringify({ no_kind: true })));
    act(() => es.emit(JSON.stringify({ kind: 42 })));
    expect(screen.queryAllByRole("listitem").length).toBe(0);
    // Still alive: a real frame after garbage gets through.
    act(() =>
      es.emit({
        kind: FRAME_KIND.EVENT_RECEIVED,
        event_id: "evt_ok",
        actor: "bob",
      }),
    );
    await waitFor(() => expect(screen.queryAllByRole("listitem").length).toBe(1));
  });

  it("falls back to empty summary for unknown frame kind (defensive default)", async () => {
    mount();
    const es = FakeEventSource.instances[0]!;
    act(() => es.open());
    act(() => es.emit(JSON.stringify({ kind: "bogus_kind" })));
    await waitFor(() => expect(screen.getAllByRole("listitem").length).toBe(1));
    // Badge shows "bogus_kind"; summary span is empty.
    expect(screen.getByText("bogus_kind")).toBeTruthy();
  });

  it("rolls older frames off when buffer size is exceeded", async () => {
    mount({ bufferSize: 3 });
    const es = FakeEventSource.instances[0]!;
    act(() => es.open());
    for (let i = 0; i < 5; i++) {
      act(() =>
        es.emit({
          kind: FRAME_KIND.CHUNK,
          event_id: "evt_1",
          text: `c${i}`,
        }),
      );
    }
    await waitFor(() => expect(screen.getAllByRole("listitem").length).toBe(3));
    expect(screen.queryByText("c0")).toBeNull();
    expect(screen.queryByText("c1")).toBeNull();
    expect(screen.getByText("c2")).toBeTruthy();
    expect(screen.getByText("c4")).toBeTruthy();
  });

  it("transitions to reconnecting on error and schedules a retry with backoff", async () => {
    vi.useFakeTimers();
    try {
      mount();
      const first = FakeEventSource.instances[0]!;
      act(() => first.open());
      act(() => first.fail());
      expect(screen.getByText(/Reconnecting…/)).toBeTruthy();
      expect(first.closed).toBe(true);
      // Backoff: base 1000 * 2 ** 1 = 2000ms before next connect.
      act(() => {
        vi.advanceTimersByTime(2000);
      });
      expect(FakeEventSource.instances.length).toBe(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("caps backoff exponent and absolute delay across many failures", async () => {
    vi.useFakeTimers();
    try {
      mount();
      for (let i = 0; i < 7; i++) {
        const es = FakeEventSource.instances[FakeEventSource.instances.length - 1]!;
        act(() => es.fail());
        act(() => {
          vi.advanceTimersByTime(15_000);
        });
      }
      expect(FakeEventSource.instances.length).toBe(8);
    } finally {
      vi.useRealTimers();
    }
  });

  it("closes the active EventSource on unmount and skips queued reconnect", () => {
    vi.useFakeTimers();
    try {
      const { unmount } = mount();
      const first = FakeEventSource.instances[0]!;
      act(() => first.fail());
      unmount();
      expect(first.closed).toBe(true);
      const before = FakeEventSource.instances.length;
      act(() => {
        vi.advanceTimersByTime(60_000);
      });
      expect(FakeEventSource.instances.length).toBe(before);
    } finally {
      vi.useRealTimers();
    }
  });
});
