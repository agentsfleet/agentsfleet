import { describe, expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { GUIDANCE } from "@/lib/events/event-summary";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { row, renderList, routerPushMock, searchParamsRef } from "./helpers/events-list-harness";

describe("EventsList — inspection and page navigation", () => {
  it("presents actor identifiers as readable names while retaining details", async () => {
    renderList({
      items: [
        row({
          event_id: "actor",
          actor: "steer:user_3GkbgXjNuJSXbdxttcWCSlPc87k",
        }),
      ],
      next_cursor: null,
    });

    expect(screen.getByText("Operator")).toBeTruthy();
    expect(
      screen.queryByText("steer:user_3GkbgXjNuJSXbdxttcWCSlPc87k"),
    ).toBeNull();
    await userEvent.click(
      screen.getByRole("button", { name: "Inspect event actor" }),
    );
    expect(screen.getAllByText("Operator")).toHaveLength(2);
    expect(
      screen.queryByText("steer:user_3GkbgXjNuJSXbdxttcWCSlPc87k"),
    ).toBeNull();
  });

  // The column used to print `01a0…7b2a` — the durable id, correct and
  // unreadable. It now carries the same callsign the fleet has on every other
  // surface, through the one component that decides how a fleet is spelled.
  it("names the fleet in the Fleet column rather than printing its id", () => {
    const fleetId = "zomb_abcdefghijkl";
    renderList({ items: [row({ fleet_id: fleetId })], next_cursor: null });
    expect(screen.getByText(agentDisplayName(fleetId))).toBeTruthy();
    expect(screen.queryByText(/zomb…ijkl/)).toBeNull();
  });

  it("fleet scope removes the Fleet column and keeps Cost, Tokens, and Duration", () => {
    renderList(
      { items: [row({ cost_nanos: "20000000" })], next_cursor: null },
      "zomb_1234567890ab",
    );
    const headers = screen
      .getAllByRole("columnheader")
      .map((header) => header.textContent);
    expect(headers).toEqual([
      "Created",
      "Tokens",
      "Duration",
      "Cost",
      "Status",
      "Details",
      "Actor",
      "Type",
      "Runs",
    ]);
    expect(screen.getByText("$0.02")).toBeTruthy();
    // A count and a row action align right with the other figures.
    for (const name of ["Runs", "Details"]) {
      expect(screen.getByRole("columnheader", { name }).className).toContain("text-right");
    }
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
    await userEvent.click(
      screen.getByRole("button", { name: "Inspect event evt_startup_1" }),
    );
    // The bodies land a tick after the dialog opens (they are fetched, not
    // carried on the row), and the request-context assertions below read them.
    await screen.findByText("Pull request");

    expect(screen.getByRole("dialog")).toBeTruthy();
    expect(screen.getByLabelText("Failed event")).toBeTruthy();
    expect(
      screen.queryByText("No specific reason was recorded for this event."),
    ).toBeNull();
    expect(screen.queryByText("startup_posture")).toBeNull();
    // No recorded cause on this row: the actionable line still shows, and the
    // "which check?" fall-back stays because nothing here names the check.
    expect(screen.getByText(GUIDANCE.STARTUP)).toBeTruthy();
    expect(screen.getByText(/did not record which check failed/)).toBeTruthy();
    expect(
      screen.queryByText(
        "Add non-empty instructions in Skill, then save the fleet.",
      ),
    ).toBeNull();
    expect(
      screen.queryByText("Make an active runner available to this workspace."),
    ).toBeNull();
    expect(
      screen.queryByText("Select an available model and provider credential."),
    ).toBeNull();
    expect(screen.queryByText(/runner logs/i)).toBeNull();
    expect(screen.getByText("Pull request")).toBeTruthy();
    expect(screen.getByText("482")).toBeTruthy();
    expect(within(screen.getByRole("dialog")).queryByRole("button", {name: /copy/i})).toBeNull();
    expect(within(screen.getByRole("dialog")).getAllByText("Created")).toHaveLength(1);
    expect(screen.queryByText("Updated")).toBeNull();
    expect(screen.queryByText(/Coordinated Universal Time/)).toBeNull();

    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("keeps every other query value when turning the page", async () => {
    // The fleet page carries its open tab in the URL. A page turn that
    // dropped it would throw the operator back to the Chat tab.
    searchParamsRef.current = new URLSearchParams("view=events");
    renderList(
      { items: [row()], next_cursor: "cur_fleet" },
      "zomb_1234567890ab",
    );
    await userEvent.click(screen.getByRole("button", { name: "Next page" }));
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    const pushed = new URLSearchParams(
      String(routerPushMock.mock.calls[0]?.[0]).split("?")[1],
    );
    expect(pushed.get("view")).toBe("events");
    expect(pushed.getAll("c")).toEqual(["cur_fleet"]);
  });

  it("renders relative <time> with a standard datetime when created_at is valid; omits when invalid", () => {
    const { container } = renderList({
      items: [
        row({ event_id: "ok", created_at: new Date(Date.UTC(2026, 0, 2, 3, 4, 0)).toISOString() }),
        row({ event_id: "bad", created_at: "not-an-instant" }),
      ],
      next_cursor: null,
    });
    const times = container.querySelectorAll("time");
    expect(times.length).toBe(1);
    expect(times[0]!.getAttribute("datetime")).toMatch(/^2026-01-02T/);
    expect(times[0]!.textContent).toMatch(/ago|^in /i);
  });

  it("walks back down the trail instead of re-asking the server", async () => {
    searchParamsRef.current = new URLSearchParams("c=cur_1&cps=25");
    renderList({ items: [row()], next_cursor: null });
    expect(screen.getByText("Page 2")).toBeTruthy();
    await userEvent.click(
      screen.getByRole("button", { name: "Previous page" }),
    );
    await waitFor(() => expect(routerPushMock).toHaveBeenCalled());
    // Dropping the last cursor is page one, which needs no cursor at all.
    expect(String(routerPushMock.mock.calls[0]?.[0])).not.toContain("c=");
  });

  it("stops at the end of the feed rather than offering an empty page", () => {
    renderList({ items: [row()], next_cursor: null });
    expect(screen.getByTestId("pagination-page")).toBeTruthy();
    expect(
      screen
        .getByRole("button", { name: "Next page" })
        .hasAttribute("disabled"),
    ).toBe(true);
  });

  it("renders tokens and duration in their columns on a row that carries them", () => {
    renderList({
      items: [row({ tokens: 12480, wall_ms: 3200 })],
      next_cursor: null,
    });
    expect(screen.getByText("12,480")).toBeTruthy();
    expect(screen.getByText("3.2s")).toBeTruthy();
  });

  it("formats sub-second duration in milliseconds", () => {
    renderList({
      items: [row({ tokens: null, wall_ms: 840 })],
      next_cursor: null,
    });
    expect(screen.getByText("840ms")).toBeTruthy();
  });

  it("renders dashes when the row carries neither figure", () => {
    renderList({
      items: [row({ tokens: null, wall_ms: null })],
      next_cursor: null,
    });
    expect(screen.getAllByText("—").length).toBe(3);
  });
});
