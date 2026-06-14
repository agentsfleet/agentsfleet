import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// ── Shared mocks ───────────────────────────────────────────────────────────

const { listZombieEventsActionMock, listWorkspaceEventsActionMock } = vi.hoisted(() => ({
  listZombieEventsActionMock: vi.fn(),
  listWorkspaceEventsActionMock: vi.fn(),
}));

vi.mock("@/app/(dashboard)/events/actions", () => ({
  listZombieEventsAction: listZombieEventsActionMock,
  listWorkspaceEventsAction: listWorkspaceEventsActionMock,
}));

// EventsList renders a next/link <Link> in its viewAllHref preview mode.
vi.mock("next/link", () => ({
  default: ({ children, ...props }: React.PropsWithChildren<React.AnchorHTMLAttributes<HTMLAnchorElement>>) =>
    React.createElement("a", props, children),
}));

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => cleanup());

// ── EventsList ─────────────────────────────────────────────────────────────

import { EventsList } from "../components/domain/EventsList";
import { FRAME_KIND, type EventRow, type EventsPage, type LiveFrame } from "@/lib/api/events";
import { TooltipProvider } from "@agentsfleet/design-system";

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
    expect(screen.getByText(/Webhooks, schedules, and manual triggers/i)).toBeTruthy();
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
    listZombieEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [row({ event_id: "p2", response_text: "page two" })],
        next_cursor: "cur_2",
      },
    });
    renderList({
      items: [row({ event_id: "p1", response_text: "page one" })],
      next_cursor: "cur_1",
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() => expect(listZombieEventsActionMock).toHaveBeenCalled());
    expect(listZombieEventsActionMock).toHaveBeenCalledWith("ws_1", "zomb_1", {
      cursor: "cur_1",
    });
    await waitFor(() => expect(screen.getByText("page two")).toBeTruthy());
  });

  it("loadMore (workspace scope) calls workspace action", async () => {
    listWorkspaceEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [row({ event_id: "wsp", response_text: "ws page" })],
        next_cursor: null,
      },
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
    await waitFor(() => expect(listWorkspaceEventsActionMock).toHaveBeenCalled());
    expect(listWorkspaceEventsActionMock).toHaveBeenCalledWith("ws_42", {
      cursor: "cur_a",
    });
  });

  it("loadMore surfaces 'Not authenticated' when the action reports unauth", async () => {
    listZombieEventsActionMock.mockResolvedValueOnce({
      ok: false,
      error: "Not authenticated",
      status: 401,
    });
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Not authenticated/),
    );
  });

  it("loadMore surfaces error message when the action returns an error", async () => {
    listZombieEventsActionMock.mockResolvedValueOnce({ ok: false, error: "backend down" });
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/backend down/),
    );
  });

  it("loadMore falls back to default message when the action returns an empty error", async () => {
    listZombieEventsActionMock.mockResolvedValueOnce({ ok: false, error: "" });
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't load more events/),
    );
  });

  it("loadMore (workspace scope) falls back to default message on empty error string", async () => {
    listWorkspaceEventsActionMock.mockResolvedValueOnce({ ok: false, error: "" });
    renderList(
      { items: [row()], next_cursor: "cur_y" },
      { kind: "workspace", workspaceId: "ws_42" },
    );
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't load more events/),
    );
  });

  // ── viewAllHref preview mode (dashboard Recent Activity) ──────────────────

  it("renders a 'View all' link to the href instead of cursor pagination when viewAllHref is set", () => {
    render(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(EventsList, {
          scope: { kind: "workspace", workspaceId: "ws_1" },
          // A next_cursor is present, so without viewAllHref the Pagination
          // control WOULD render — this proves the preview branch replaces it.
          initial: { items: [row({ event_id: "p1" })], next_cursor: "cur_1" },
          viewAllHref: "/events",
        }),
      ),
    );
    expect(screen.getByRole("link", { name: /view all events/i }).getAttribute("href")).toBe("/events");
    expect(screen.queryByRole("button", { name: /load more|next/i })).toBeNull();
  });

  it("hides the 'View all' link in preview mode when no further events exist (next_cursor null)", () => {
    render(
      React.createElement(
        TooltipProvider,
        null,
        React.createElement(EventsList, {
          scope: { kind: "workspace", workspaceId: "ws_1" },
          // Every event already fits in the preview (no next_cursor), so a
          // "View all" link would point at an identical list — it must not show.
          initial: { items: [row({ event_id: "p1" })], next_cursor: null },
          viewAllHref: "/events",
        }),
      ),
    );
    expect(screen.queryByRole("link", { name: /view all events/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /load more|next/i })).toBeNull();
  });

  it("renders cursor pagination (not a 'View all' link) when viewAllHref is absent", () => {
    renderList({ items: [row({ event_id: "p1" })], next_cursor: "cur_1" });
    expect(screen.getByRole("button", { name: /load more|next/i })).toBeTruthy();
    expect(screen.queryByRole("link", { name: /view all events/i })).toBeNull();
  });
});
