import React from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

// ── Shared mocks ───────────────────────────────────────────────────────────

// Paging is URL state now: the pager navigates and the Server Component
// fetches the page the cursor names, so these assert the URL that gets
// pushed rather than a client-side fetch.
const { routerPushMock, searchParamsRef } = vi.hoisted(() => ({
  routerPushMock: vi.fn(),
  searchParamsRef: { current: new URLSearchParams() },
}));

vi.mock("next/navigation", () => ({
  useRouter: () => ({ push: routerPushMock }),
  usePathname: () => "/w/ws_1/events",
  useSearchParams: () => searchParamsRef.current,
}));

beforeEach(() => {
  vi.clearAllMocks();
  searchParamsRef.current = new URLSearchParams();
});

afterEach(() => cleanup());

// ── EventsList ─────────────────────────────────────────────────────────────

import { EventsList } from "../components/domain/EventsList";
import { type EventRow, type EventsPage } from "@/lib/api/events";
import { TooltipProvider } from "@agentsfleet/design-system";
import { GUIDANCE } from "@/lib/events/event-summary";

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
    failure_detail: null,
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
      React.createElement(EventsList, { initial, fleetId }),
    ),
  );
}

describe("EventsList — the standard workspace events table", () => {
  it("lets the event log grow with the page instead of a fixed-height scroll box", () => {
    // Regression for the dead-space report: the table's default bound is a
    // constant 384px, so a 50-row first fetch rendered ~8 rows above a screen
    // of black space and "Load more" appended rows nobody could see.
    const { container } = renderList({
      items: [row({ event_id: "evt_grow_1" })],
      next_cursor: "cursor-1",
    });
    expect(container.querySelector(".max-h-none")).toBeTruthy();
  });

  it("collapses repeated identical failures into one row that opens on demand", async () => {
    const failure = (id: string) =>
      row({
        event_id: id,
        status: "fleet_error",
        response_text: null,
        failure_label: "startup_posture",
        failure_detail: "no instructions configured",
      });
    renderList({ items: [failure("f1"), failure("f2"), failure("f3")], next_cursor: null });

    // Three deliveries, one row, and the count says so.
    expect(screen.getByText("×3")).toBeTruthy();
    expect(screen.getAllByRole("button", { name: /Inspect event/ })).toHaveLength(1);

    // The count is a control, not a claim: it opens to the rows it covers.
    await userEvent.click(screen.getByRole("button", { name: /Expand 3 repeated failures/ }));
    expect(screen.getAllByRole("button", { name: /Inspect event/ })).toHaveLength(3);

    // ...and closes again, so the operator can put the wall of repeats away.
    await userEvent.click(screen.getByRole("button", { name: /Collapse 3 repeated failures/ }));
    expect(screen.getAllByRole("button", { name: /Inspect event/ })).toHaveLength(1);
  });

  it("leaves successes alone however alike they look", () => {
    renderList({
      items: [
        row({ event_id: "s1", status: "processed", response_text: "reviewed #1", failure_label: null }),
        row({ event_id: "s2", status: "processed", response_text: "reviewed #2", failure_label: null }),
      ],
      next_cursor: null,
    });
    // Two successful runs are two pieces of work — collapsing them would hide
    // what the fleet actually did.
    expect(screen.queryByText(/^×/)).toBeNull();
    expect(screen.getAllByRole("button", { name: /Inspect event/ })).toHaveLength(2);
  });

  it("dims a failed run's zero metrics but never a successful run's", () => {
    const { container } = renderList({
      items: [
        row({ event_id: "z1", status: "fleet_error", response_text: null, tokens: 0, wall_ms: 0, cost_nanos: 0 }),
        row({ event_id: "z2", status: "processed", tokens: 0, wall_ms: 0, cost_nanos: 0, failure_label: null }),
      ],
      next_cursor: null,
    });
    // A failed run's zero reports that nothing ran; a successful run's zero is
    // a genuine result and keeps full weight.
    const dimmed = container.querySelectorAll(".text-muted-foreground\\/50");
    expect(dimmed.length).toBe(3);
  });

  it("renders default empty state when no items", () => {
    renderList({ items: [], next_cursor: null });
    expect(screen.getByText(/No events yet/i)).toBeTruthy();
    expect(screen.getByText(/Fleet activity appears here/i)).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
    // No pagination affordance on an empty feed — nothing to page through.
    expect(screen.queryByRole("button", { name: /load more|next/i })).toBeNull();
  });

  it("still offers a way forward when an empty page carries a cursor (no stranded data)", () => {
    renderList({ items: [], next_cursor: "cur_more" });
    // Compaction between pages can return an empty page with a live cursor —
    // the affordance to keep paging must survive the empty state.
    expect(screen.getByText(/No events yet/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" })).toBeTruthy();
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
    // Index 7, not 6: the leading Runs column shifted every cell along.
    const summaryCell = cells[7];
    expect(summaryCell?.textContent).toBe("No result recorded");
  });

  it("sorts every event data column from its header arrow", () => {
    renderList({
      items: [
        row({ event_id: "a", response_text: "response", cost_nanos: 5, tokens: 4, wall_ms: 3 }),
        row({ event_id: "b", response_text: null, failure_label: "oom_kill", cost_nanos: null, tokens: null, wall_ms: null }),
        row({ event_id: "c", response_text: null, failure_label: null }),
      ],
      next_cursor: null,
    });

    for (const name of ["Time", "Status", "Fleet", "Actor", "Type", "Result", "Cost", "Tokens", "Duration"]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("columnheader", { name }).getAttribute("aria-sort")).not.toBe("none");
    }
  });

  it("sorts by the Runs column across grouped and single rows", () => {
    // Exercises the Runs sortValue on both a group lead (a real count) and a
    // standalone row (no count → 0), so the count/no-count branch is covered.
    const failure = (id: string) =>
      row({
        event_id: id,
        status: "fleet_error",
        response_text: null,
        failure_label: "startup_posture",
        failure_detail: "no instructions configured",
      });
    renderList({
      items: [
        failure("g1"),
        failure("g2"),
        row({ event_id: "solo", status: "processed", response_text: "done", failure_label: null }),
      ],
      next_cursor: null,
    });
    fireEvent.click(screen.getByRole("button", { name: "Runs" }));
    expect(screen.getByRole("columnheader", { name: "Runs" }).getAttribute("aria-sort")).not.toBe("none");
  });

  it("sorts results by the normalized operator-facing summary", () => {
    renderList({
      items: [
        row({ event_id: "failure", response_text: null, failure_label: "oom_kill" }),
        row({ event_id: "response", response_text: "  Quiet   result ", failure_label: null }),
      ],
      next_cursor: null,
    });

    fireEvent.click(screen.getByRole("button", { name: "Result" }));
    expect(screen.getAllByRole("row")[1]?.textContent).toContain("Quiet result");
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
    expect(screen.getByRole("button", { name: "Inspect event f" }).className).toContain("min-h-11");
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
      "Runs",
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
    // No recorded cause on this row: the actionable line still shows, and the
    // "which check?" fall-back stays because nothing here names the check.
    expect(screen.getByText(GUIDANCE.STARTUP)).toBeTruthy();
    expect(screen.getByText(/did not record which check failed/)).toBeTruthy();
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

  it("keeps every other query value when turning the page", async () => {
    // The fleet page carries its open tab in the URL. A page turn that
    // dropped it would throw the operator back to the Chat tab.
    searchParamsRef.current = new URLSearchParams("view=events");
    renderList({ items: [row()], next_cursor: "cur_fleet" }, "zomb_1234567890ab");
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    const pushed = new URLSearchParams(String(routerPushMock.mock.calls[0]?.[0]).split("?")[1]);
    expect(pushed.get("view")).toBe("events");
    expect(pushed.getAll("c")).toEqual(["cur_fleet"]);
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

  it("test_events_table_paginates_by_cursor — the page turn lands in the URL", async () => {
    renderList({ items: [row({ event_id: "p1", response_text: "page one" })], next_cursor: "cur_1" });
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    // The cursor is appended to the trail, so the server fetches page two and
    // Back walks the operator to page one.
    expect(String(routerPushMock.mock.calls[0]?.[0])).toContain("c=cur_1");
  });

  it("walks back down the trail instead of re-asking the server", async () => {
    searchParamsRef.current = new URLSearchParams("c=cur_1");
    renderList({ items: [row()], next_cursor: null });
    expect(screen.getByText("Page 2")).toBeTruthy();
    await userEvent.click(screen.getByRole("button", { name: "Previous page" }));
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    // Dropping the last cursor is page one, which needs no cursor at all.
    expect(String(routerPushMock.mock.calls[0]?.[0])).not.toContain("c=");
  });

  it("stops at the end of the feed rather than offering an empty page", () => {
    renderList({ items: [row()], next_cursor: null });
    // A single page needs no pager at all.
    expect(screen.queryByRole("button", { name: "Next page" })).toBeNull();
  });

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
