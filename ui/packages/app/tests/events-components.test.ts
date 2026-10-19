import { describe, expect, it } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { row, renderList } from "./helpers/events-list-harness";

describe("EventsList — the standard workspace events table", () => {
  it("collapses repeated identical failures into one row that opens on demand", async () => {
    const failure = (id: string) =>
      row({
        event_id: id,
        status: "fleet_error",
        response_text: null,
        failure_label: "startup_posture",
        failure_detail: "no instructions configured",
      });
    renderList({
      items: [failure("f1"), failure("f2"), failure("f3")],
      next_cursor: null,
    });

    // Three deliveries, one row, and the count says so.
    expect(screen.getByText("×3")).toBeTruthy();
    expect(
      screen.getAllByRole("button", { name: /Inspect event/ }),
    ).toHaveLength(1);

    // The count is a control, not a claim: it opens to the rows it covers.
    await userEvent.click(
      screen.getByRole("button", { name: /Expand 3 repeated failures/ }),
    );
    expect(
      screen.getAllByRole("button", { name: /Inspect event/ }),
    ).toHaveLength(3);

    // ...and closes again, so the operator can put the wall of repeats away.
    await userEvent.click(
      screen.getByRole("button", { name: /Collapse 3 repeated failures/ }),
    );
    expect(
      screen.getAllByRole("button", { name: /Inspect event/ }),
    ).toHaveLength(1);
  });

  it("leaves successes alone however alike they look", () => {
    renderList({
      items: [
        row({
          event_id: "s1",
          status: "processed",
          response_text: "reviewed #1",
          failure_label: null,
        }),
        row({
          event_id: "s2",
          status: "processed",
          response_text: "reviewed #2",
          failure_label: null,
        }),
      ],
      next_cursor: null,
    });
    // Two successful runs are two pieces of work — collapsing them would hide
    // what the fleet actually did.
    expect(screen.queryByText(/^×/)).toBeNull();
    expect(
      screen.getAllByRole("button", { name: /Inspect event/ }),
    ).toHaveLength(2);
  });

  it("dims a failed run's zero metrics but never a successful run's", () => {
    const { container } = renderList({
      items: [
        row({
          event_id: "z1",
          status: "fleet_error",
          response_text: null,
          tokens: 0,
          wall_ms: 0,
          cost_nanos: "0",
        }),
        row({
          event_id: "z2",
          status: "processed",
          tokens: 0,
          wall_ms: 0,
          cost_nanos: "0",
          failure_label: null,
        }),
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
    expect(screen.getByText(/No activity yet/i)).toBeTruthy();
    expect(screen.getByText(/Fleet activity appears here/i)).toBeTruthy();
    expect(screen.queryByRole("table")).toBeNull();
    // No pagination affordance on an empty feed — nothing to page through.
    expect(
      screen.queryByRole("button", { name: /load more|next/i }),
    ).toBeNull();
  });

  it("still offers a way forward when an empty page carries a cursor (no stranded data)", () => {
    renderList({ items: [], next_cursor: "cur_more" });
    // Compaction between pages can return an empty page with a live cursor —
    // the affordance to keep paging must survive the empty state.
    expect(screen.getByText(/No activity yet/i)).toBeTruthy();
    expect(screen.getByRole("button", { name: "Next page" })).toBeTruthy();
  });

  it("test_events_page_uses_standard_table — one table row per event with status badge, actor, and summary", () => {
    renderList({
      items: [
        row({
          event_id: "a",
          status: "processed",
          response_text: "first event",
        }),
        row({
          event_id: "b",
          status: "fleet_error",
          response_text: null,
          failure_label: "boom",
        }),
        row({ event_id: "c", status: "gate_blocked", response_text: null }),
        row({ event_id: "d", status: "received", response_text: "rec" }),
        row({
          event_id: "e",
          status: "weird-unknown",
          response_text: "fallback variant",
        }),
      ],
      next_cursor: null,
    });
    // The standard table primitive: a real <table> with a header row.
    expect(screen.getByRole("table")).toBeTruthy();
    // 1 header row + 5 event rows.
    expect(screen.getAllByRole("row").length).toBe(6);
    // The reply is NOT on a list row any more: the list read carries no
    // bodies, and no column tries to explain a failure either — Status flags
    // it and Inspect carries the reason.
    expect(screen.queryByText("first event")).toBeNull();
    expect(screen.queryByText("No failure recorded")).toBeNull();
    // No failure text on a list row at all now — not the friendly sentence and
    // not the raw tag. "boom" is not a real FailureClass, and the list is the
    // wrong place to find that out; Inspect is.
    expect(screen.queryByText("boom")).toBeNull();
    // weird status falls through to default badge variant (still rendered)
    expect(screen.getByText("weird-unknown")).toBeTruthy();
    // A row with neither response text nor a failure label says so instead of
    // leaving the operator to guess whether the interface lost data.
    const rowC = screen
      .getAllByRole("row")
      .find((r) => r.textContent?.includes("gate_blocked"));
    expect(rowC).toBeTruthy();
    const cells = Array.from(rowC!.querySelectorAll("td"));
    // Fleet, Time, Tokens, Duration, Cost, Status, Details, Actor, then Type
    // at index 8; Runs closes the row. No cell restates the failure.
    expect(cells[8]?.textContent).toBe("chat");
  });

  it("sorts every event data column from its header arrow", () => {
    renderList({
      items: [
        row({
          event_id: "a",
          response_text: "response",
          cost_nanos: "5",
          tokens: 4,
          wall_ms: 3,
        }),
        row({
          event_id: "b",
          response_text: null,
          failure_label: "oom_kill",
          cost_nanos: null,
          tokens: null,
          wall_ms: null,
        }),
        row({ event_id: "c", response_text: null, failure_label: null }),
      ],
      next_cursor: null,
    });

    for (const name of [
      "Created",
      "Status",
      "Fleet",
      "Actor",
      "Type",
      "Cost",
      "Tokens",
      "Duration",
    ]) {
      fireEvent.click(screen.getByRole("button", { name }));
      expect(
        screen.getByRole("columnheader", { name }).getAttribute("aria-sort"),
      ).not.toBe("none");
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
        row({
          event_id: "solo",
          status: "processed",
          response_text: "done",
          failure_label: null,
        }),
      ],
      next_cursor: null,
    });
    fireEvent.click(screen.getByRole("button", { name: "Runs" }));
    expect(
      screen
        .getByRole("columnheader", { name: "Runs" })
        .getAttribute("aria-sort"),
    ).not.toBe("none");
  });

  it("keeps every failure explanation off the list row, tag and sentence alike", () => {
    // Status flags the failure and Inspect explains it, with the recorded cause
    // and the fix hint the list could never fit. `tests/event-details-dialog`
    // owns the copy itself; this only proves the list stays out of it.
    renderList({
      items: [
        row({
          event_id: "f",
          status: "fleet_error",
          response_text: "Engine configuration could not be assembled.",
          failure_label: "oom_kill",
        }),
        row({
          event_id: "g",
          status: "gate_blocked",
          response_text: null,
          failure_label: "budget_breach",
        }),
      ],
      next_cursor: null,
    });
    for (const leaked of [
      "Ran out of memory",
      "Fleet budget limit reached",
      "oom_kill",
      "budget_breach",
      "Engine configuration could not be assembled.",
    ]) {
      expect(screen.queryByText(leaked)).toBeNull();
    }
    // The route to the reason is the row's own Inspect control.
    const inspect = screen.getByRole("button", { name: "Inspect event f" });
    expect(inspect).toBeTruthy();
    expect(inspect.className).toContain("min-h-11");
  });

});
