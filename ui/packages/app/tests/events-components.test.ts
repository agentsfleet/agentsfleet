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
    // A row with neither response text nor a failure label says so instead of
    // leaving the operator to guess whether the interface lost data.
    const rowC = screen.getAllByRole("row").find((r) => r.textContent?.includes("gate_blocked"));
    expect(rowC).toBeTruthy();
    const cells = Array.from(rowC!.querySelectorAll("td"));
    const summaryCell = cells[6];
    expect(summaryCell?.textContent).toBe("No result recorded");
  });

  it("renders a known FailureClass tag as a friendly label without its internal tag", () => {
    renderList({
      items: [
        row({ event_id: "f", status: "fleet_error", response_text: null, failure_label: "oom_kill" }),
        row({ event_id: "g", status: "fleet_error", response_text: null, failure_label: "resource_kill" }),
      ],
      next_cursor: null,
    });
    expect(screen.getByText("Ran out of memory")).toBeTruthy();
    expect(screen.getByText("Hit a resource limit")).toBeTruthy();
    expect(screen.queryByText("oom_kill")).toBeNull();
    expect(screen.queryByText("resource_kill")).toBeNull();
    expect(screen.getByRole("button", { name: "Inspect event f" })).toBeTruthy();
  });

  it("keeps a recorded response visible without exposing its internal failure tag", () => {
    renderList({
      items: [row({
        event_id: "response-failure",
        status: "fleet_error",
        response_text: "Engine configuration could not be assembled.",
        failure_label: "startup_posture",
      })],
      next_cursor: null,
    });
    expect(screen.getByText("Engine configuration could not be assembled.")).toBeTruthy();
    expect(screen.queryByText("startup_posture")).toBeNull();
  });

  it("presents actor identifiers as readable names while retaining details", async () => {
    renderList({
      items: [row({ event_id: "actor", actor: "steer:user_3GkbgXjNuJSXbdxttcWCSlPc87k" })],
      next_cursor: null,
    });

    expect(screen.getByText("Operator")).toBeTruthy();
    expect(screen.queryByText("steer:user_3GkbgXjNuJSXbdxttcWCSlPc87k")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Inspect event actor" }));
    expect(screen.getAllByText("Operator")).toHaveLength(2);
    expect(screen.queryByText("steer:user_3GkbgXjNuJSXbdxttcWCSlPc87k")).toBeNull();
  });

  it("renders the fleet budget failure with operator-facing copy", () => {
    renderList({
      items: [row({ event_id: "budget", status: "gate_blocked", response_text: null, failure_label: "budget_breach" })],
      next_cursor: null,
    });
    expect(screen.getByText("Fleet budget limit reached")).toBeTruthy();
    expect(screen.queryByText("budget_breach")).toBeNull();
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
    expect(headers).toEqual([
      "Time",
      "Status",
      "Actor",
      "Details",
      "Type",
      "Result",
      "Cost",
      "Tokens",
      "Duration",
    ]);
    expect(screen.getByText("$0.02")).toBeTruthy();
  });

  it("opens actionable diagnostics when startup safety failed without a recorded reason", async () => {
    renderList({
      items: [
        row({
          event_id: "evt_startup_1",
          actor: "github-app",
          event_type: "webhook",
          status: "fleet_error",
          request_json: '{"action":"opened","pull_request":482}',
          response_text: null,
          failure_label: "startup_posture",
        }),
      ],
      next_cursor: null,
    });

    expect(screen.queryByRole("dialog")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Inspect event evt_startup_1" }));

    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByLabelText("Failed event")).toBeTruthy();
    expect(screen.queryByText("No specific reason was recorded for this event.")).toBeNull();
    expect(screen.queryByText("startup_posture")).toBeNull();
    expect(screen.getByText(
      "Nothing specific can be fixed from this event because it did not record which startup check failed.",
    )).toBeTruthy();
    expect(screen.getByText(
      "Retry it once. If it fails again, use the copy icon below and ask a coding agent to inspect the diagnostic.",
    )).toBeTruthy();
    expect(screen.queryByText("Add non-empty instructions in Skill, then save the fleet.")).toBeNull();
    expect(screen.queryByText("Make an active runner available to this workspace.")).toBeNull();
    expect(screen.queryByText("Select an available model and provider credential.")).toBeNull();
    expect(screen.queryByText(/runner logs/i)).toBeNull();
    expect(screen.getByText("Pull request")).toBeTruthy();
    expect(screen.getByText("482")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy event ID" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Copy diagnostic" })).toBeTruthy();
    expect(screen.queryByText("Copy diagnostic")).toBeNull();
    expect(screen.queryByText("Copy event details")).toBeNull();
    expect(screen.getAllByText("Created")).toHaveLength(1);
    expect(screen.queryByText("Updated")).toBeNull();
    expect(screen.queryByText(/Coordinated Universal Time/)).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
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

  it("renders relative <time> with a standard datetime when created_at is valid; omits when invalid", () => {
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
    expect(times[0]!.textContent).toMatch(/ago|^in /i);
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
