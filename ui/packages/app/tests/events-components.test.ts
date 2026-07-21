import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// ── Shared mocks ───────────────────────────────────────────────────────────

const { listWorkspaceEventsActionMock } = vi.hoisted(() => ({
  listWorkspaceEventsActionMock: vi.fn(),
}));

vi.mock("@/app/(dashboard)/w/[workspaceId]/events/actions", () => ({
  listWorkspaceEventsAction: listWorkspaceEventsActionMock,
}));

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(() => cleanup());

// ── EventsList ─────────────────────────────────────────────────────────────

import { EventsList } from "../components/domain/EventsList";
import { type EventRow, type EventsPage } from "@/lib/api/events";
import { TooltipProvider } from "@agentsfleet/design-system";

function row(over: Partial<EventRow> = {}): EventRow {
  const now = Date.UTC(2026, 3, 28, 10, 30, 0);
  return {
    event_id: "evt_1",
    fleet_id: "zomb_1234567890ab",
    workspace_id: "ws_1",
    actor: "alice@example.com",
    event_type: "chat",
    status: "processed",
    request_json: "{}",
    response_text: "hello world",
    tokens: 1,
    wall_ms: 10,
    cost_nanos: null,
    failure_label: null,
    checkpoint_id: null,
    resumes_event_id: null,
    created_at: now,
    updated_at: now,
    ...over,
  };
}

function renderList(initial: EventsPage, fleetId?: string) {
  return render(
    React.createElement(
      TooltipProvider,
      null,
      React.createElement(EventsList, { workspaceId: "ws_1", initial, fleetId }),
    ),
  );
}

describe("EventsList — the standard workspace events table", () => {
  it("renders default empty state when no items", () => {
    renderList({ items: [], next_cursor: null });
    expect(screen.getByText(/No events yet/i)).toBeTruthy();
    expect(screen.getByText(/Fleet activity appears here/i)).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
    // No pagination affordance on an empty feed — nothing to page through.
    expect(screen.queryByRole("button", { name: /load more|next/i })).toBeNull();
  });

  it("still offers Load more when an empty page carries a cursor (no stranded data)", () => {
    renderList({ items: [], next_cursor: "cur_more" });
    // Compaction between pages can return an empty page with a live cursor —
    // the affordance to keep paging must survive the empty state.
    expect(screen.getByText(/No events yet/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: /load more|next/i })).toBeTruthy();
  });

  it("test_events_page_uses_standard_table — one table row per event with status badge, actor, and summary", () => {
    renderList({
      items: [
        row({ event_id: "a", status: "processed", response_text: "first event" }),
        row({ event_id: "b", status: "fleet_error", response_text: null, failure_label: "boom" }),
        row({ event_id: "c", status: "gate_blocked", response_text: null }),
        row({ event_id: "d", status: "received", response_text: "rec" }),
        row({ event_id: "e", status: "weird-unknown", response_text: "fallback variant" }),
      ],
      next_cursor: null,
    });
    // The standard table primitive: a real <table> with a header row.
    expect(screen.getByRole("table")).toBeTruthy();
    // 1 header row + 5 event rows.
    expect(screen.getAllByRole("row").length).toBe(6);
    expect(screen.getByText("first event")).toBeTruthy();
    // failure_label fallback for null response_text — "boom" isn't a real
    // FailureClass tag, so it renders raw (fails soft on unknown tags).
    expect(screen.getByText("boom")).toBeTruthy();
    // weird status falls through to default badge variant (still rendered)
    expect(screen.getByText("weird-unknown")).toBeTruthy();
    // A row with neither response text nor a failure label renders an empty
    // summary cell — never a placeholder or a crash. Summary is the 6th column.
    const rowC = screen.getAllByRole("row").find((r) => r.textContent?.includes("gate_blocked"));
    expect(rowC).toBeTruthy();
    const cells = Array.from(rowC!.querySelectorAll("td"));
    const summaryCell = cells[5];
    expect(summaryCell?.textContent).toBe("");
  });

  it("renders a known FailureClass tag as its friendly label, not the raw enum name", () => {
    renderList({
      items: [row({ event_id: "f", status: "fleet_error", response_text: null, failure_label: "oom_kill" })],
      next_cursor: null,
    });
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.queryByText(/oom_kill/)).toBeNull();
  });

  it("collapses whitespace and truncates long summary text to 160 chars", () => {
    const long = "x".repeat(300);
    renderList({
      items: [row({ event_id: "z", response_text: `  multi  \n  line   ${long}` })],
      next_cursor: null,
    });
    const cell = screen.getByTitle(/multi/);
    const txt = cell.textContent ?? "";
    expect(txt.length).toBeLessThanOrEqual(161); // 157 + "…"
    expect(txt.endsWith("…")).toBe(true);
    expect(txt).not.toMatch(/\s\s/);
  });

  it("renders the short fleet id in the Fleet column", () => {
    renderList({
      items: [row({ fleet_id: "zomb_abcdefghijkl" })],
      next_cursor: null,
    });
    // shortId: first 4 + … + last 4
    expect(screen.getByText(/zomb…ijkl/)).toBeTruthy();
  });

  it("shortId returns the id verbatim when length <= 12", () => {
    renderList({
      items: [row({ fleet_id: "abc12345" })],
      next_cursor: null,
    });
    expect(screen.getByText("abc12345")).toBeTruthy();
  });

  it("fleet scope removes the Fleet column and keeps Result, Cost, Tokens, and Duration", () => {
    renderList({ items: [row({ cost_nanos: 20_000_000 })], next_cursor: null }, "zomb_1234567890ab");
    const headers = screen.getAllByRole("columnheader").map((header) => header.textContent);
    expect(headers).toEqual(["Time", "Status", "Actor", "Type", "Result", "Cost", "Tokens", "Duration"]);
    expect(screen.getByText("$0.02")).toBeTruthy();
  });

  it("fleet-scoped pagination preserves the fleet filter", async () => {
    listWorkspaceEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: { items: [], next_cursor: null },
    });
    renderList({ items: [row()], next_cursor: "cur_fleet" }, "zomb_1234567890ab");
    await userEvent.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() => expect(listWorkspaceEventsActionMock).toHaveBeenCalledWith("ws_1", {
      cursor: "cur_fleet",
      fleet_id: "zomb_1234567890ab",
    }));
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

  it("test_events_table_paginates_by_cursor — load more appends rows and updates the cursor", async () => {
    listWorkspaceEventsActionMock.mockResolvedValueOnce({
      ok: true,
      data: {
        items: [row({ event_id: "p2", response_text: "page two" })],
        next_cursor: null,
      },
    });
    renderList({
      items: [row({ event_id: "p1", response_text: "page one" })],
      next_cursor: "cur_1",
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() => expect(listWorkspaceEventsActionMock).toHaveBeenCalled());
    expect(listWorkspaceEventsActionMock).toHaveBeenCalledWith("ws_1", { cursor: "cur_1" });
    await waitFor(() => expect(screen.getByText("page two")).toBeTruthy());
    expect(screen.getByText("page one")).toBeTruthy();
  });

  it("loadMore surfaces 'Not authenticated' when the action reports unauth", async () => {
    listWorkspaceEventsActionMock.mockResolvedValueOnce({
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
    listWorkspaceEventsActionMock.mockResolvedValueOnce({ ok: false, error: "backend down" });
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/backend down/),
    );
  });

  it("loadMore falls back to default message when the action returns an empty error", async () => {
    listWorkspaceEventsActionMock.mockResolvedValueOnce({ ok: false, error: "" });
    renderList({ items: [row()], next_cursor: "cur_x" });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: /load more|next/i }));
    await waitFor(() =>
      expect(screen.getByRole("alert").textContent).toMatch(/Couldn't load more events/),
    );
  });

  // ── The event's cost columns ───────────────────────────────────────────────
  //
  // tokens and wall_ms ride the wire on every EventRow; the table gives each
  // its own labelled column, and a row without them renders a dash — an unknown
  // is never a fabricated zero.

  it("renders tokens and duration in their columns on a row that carries them", () => {
    renderList({ items: [row({ tokens: 12480, wall_ms: 3200 })], next_cursor: null });
    expect(screen.getByText("12,480")).toBeTruthy();
    expect(screen.getByText("3.2s")).toBeTruthy();
  });

  it("formats sub-second duration in milliseconds", () => {
    renderList({ items: [row({ tokens: null, wall_ms: 840 })], next_cursor: null });
    expect(screen.getByText("840ms")).toBeTruthy();
  });

  it("renders dashes when the row carries neither figure", () => {
    renderList({ items: [row({ tokens: null, wall_ms: null })], next_cursor: null });
    expect(screen.getAllByText("—").length).toBe(3);
  });
});
