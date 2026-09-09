import {
  AGENTSFLEET_B,
  AGENT_A_DISPLAY_NAME,
  AGENT_B_DISPLAY_NAME,
  WORKSPACE_ID,
  gate,
  render,
} from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";

/** The header cell for a sortable column is a button carrying its name. */
function sortBy(header: string) {
  fireEvent.click(screen.getByRole("button", { name: new RegExp(`^${header}$`, "i") }));
}

/** Fleet names in the order the table currently lists them. */
function fleetOrder(): string[] {
  return Array.from(document.querySelectorAll("[data-agent-name]"))
    .map((el) => el.getAttribute("data-agent-name") ?? "")
    .filter((name) => name.startsWith("Agent") || name === "Deleted agent");
}

const NEWER = 1_760_000_200_000;
const OLDER = 1_760_000_100_000;
/** A short gap after an instant — a decision that followed its request. */
const A_MOMENT_MS = 1_000;
/** A longer one, so the deadline sorts clear of every decision instant. */
const A_WHILE_MS = 5_000;

function twoRows() {
  return [
    gate({
      gate_id: "01999999-0000-7000-8000-000000000001",
      action_id: "a1",
      fleet_id: AGENTSFLEET_B,
      created_at: NEWER,
      status: "approved",
      updated_at: NEWER + A_MOMENT_MS,
      resolved_by: "human:someone",
    }),
    gate({
      gate_id: "01999999-0000-7000-8000-000000000002",
      action_id: "a2",
      created_at: OLDER,
      // Still pending: no `updated_at`, so the Decided column sorts on the
      // deadline instead. Both arms of that `??` need a row.
      updated_at: null,
      timeout_at: NEWER + A_WHILE_MS,
    }),
  ];
}

describe("ApprovalsTable — sorting", () => {
  it("sorts by fleet name, not by fleet id", () => {
    // The id is a uuid; sorting on it would order the table by entropy. The
    // column sorts on the derived callsign the operator can actually read.
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: twoRows(),
        initialCursor: null,
      }),
    );

    sortBy("Fleet");
    const ascending = fleetOrder();
    expect(ascending).toEqual([...ascending].sort());
    expect(ascending).toContain(AGENT_A_DISPLAY_NAME);
    expect(ascending).toContain(AGENT_B_DISPLAY_NAME);

    sortBy("Fleet");
    expect(fleetOrder()).toEqual([...ascending].reverse());
  });

  it("sorts by when the request arrived", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: twoRows(),
        initialCursor: null,
      }),
    );

    sortBy("Requested");
    // Oldest request first once the column is sorted ascending; the row that
    // arrived later follows it.
    expect(fleetOrder()).toEqual([AGENT_A_DISPLAY_NAME, AGENT_B_DISPLAY_NAME]);
    sortBy("Requested");
    expect(fleetOrder()).toEqual([AGENT_B_DISPLAY_NAME, AGENT_A_DISPLAY_NAME]);
  });

  it("sorts a pending row by its deadline and a settled one by its answer", () => {
    // `updated_at ?? timeout_at`: a row nobody has answered has no decision
    // instant, so the column falls back to when it will be taken away.
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: twoRows(),
        initialCursor: null,
      }),
    );

    sortBy("Decided");
    const ascending = fleetOrder();
    expect(ascending).toHaveLength(2);
    sortBy("Decided");
    expect(fleetOrder()).toEqual([...ascending].reverse());
  });

  it("sorts by status label", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: twoRows(),
        initialCursor: null,
      }),
    );

    sortBy("Status");
    // "Approved" before "Pending" alphabetically, and the fleet rows follow it.
    expect(fleetOrder()).toEqual([AGENT_B_DISPLAY_NAME, AGENT_A_DISPLAY_NAME]);
  });
});
