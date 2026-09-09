import {
  AGENTSFLEET_B,
  AGENT_A_DISPLAY_NAME,
  AGENT_B_DISPLAY_NAME,
  WORKSPACE_ID,
  gate,
  listApprovalsActionMock,
  render,
} from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";

describe("ApprovalsList — EmptyState", () => {
  // The server renders EVERY state now, so an empty table is an empty inbox
  // from the first paint. There is no read in flight to hold the verdict for.
  //
  // It used to render the PENDING page only, which made an empty mount a
  // question rather than an answer: a workspace whose approvals were all
  // settled showed "No approvals yet", then replaced it when four more reads
  // landed. A placeholder covered that window; deleting the window deleted the
  // need for it.
  it("says the inbox is empty immediately, with no read and no placeholder", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
      }),
    );
    expect(screen.getByText(/no approvals yet/i)).toBeTruthy();
    expect(screen.queryByTestId("approvals-loading")).toBeNull();
    expect(listApprovalsActionMock).not.toHaveBeenCalled();
  });

  it("reads nothing on mount when the server already handed it rows", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    // The whole cost the redesign removed: a client read on every mount, which
    // Next queued behind its own token mint before the table could complete.
    expect(listApprovalsActionMock).not.toHaveBeenCalled();
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
  });
});

describe("ApprovalsList — initial render", () => {
  it("renders fleet name, gate kind badge, and approve/deny buttons per row", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate()],
        initialCursor: null,
      }),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    expect(screen.getByText("destructive_action")).toBeTruthy();
    expect(screen.getByRole("button", { name: /^approve:/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^deny:/i })).toBeTruthy();
    // Details dropped with the redesign: the request title is the link to the
    // gate page, so the row still reaches the evidence and the reason box.
    const title = screen.getByRole("link", { name: /open pr titled x/i });
    expect(title.getAttribute("href")).toContain("/approvals/");
  });

  it("renders one card per item", () => {
    const items = [
      gate({ gate_id: "01999999-0000-7000-8000-000000000001", action_id: "a1" }),
      gate({
        gate_id: "01999999-0000-7000-8000-000000000002",
        action_id: "a2",
        fleet_name: "approvals-b",
        fleet_id: AGENTSFLEET_B,
      }),
    ];
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: items,
        initialCursor: null,
      }),
    );
    expect(screen.getByText(AGENT_A_DISPLAY_NAME)).toBeTruthy();
    expect(screen.getByText(AGENT_B_DISPLAY_NAME)).toBeTruthy();
  });
});

describe("ApprovalsList — gate card fallbacks", () => {
  it("falls back to tool:action and omits optional chrome when fields are empty", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ proposed_action: "", gate_kind: "", blast_radius: "" })],
        initialCursor: null,
      }),
    );
    // proposed_action "" → `${tool_name}:${action_name}` fallback in the title link.
    expect(screen.getByText("write_repo:create_pr")).toBeTruthy();
    // gate_kind "" → no kind badge; blast_radius "" → no blast-radius content.
    expect(screen.queryByText("destructive_action")).toBeNull();
    expect(screen.queryByText("single repo branch")).toBeNull();
  });
});

describe("ApprovalsTable — states this build has no arm for", () => {
  // A status the dashboard does not know is shown verbatim rather than guessed
  // at or hidden: an unrecognised row is one somebody still has to understand,
  // and silently calling it pending would be the worse of the two mistakes.
  it("shows an unknown status as itself, with the neutral badge", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ status: "quarantined" })],
        initialCursor: null,
      }),
    );
    expect(screen.getByText("quarantined")).toBeTruthy();
    // Not mapped onto one of the five, so no "Pending"/"Approved" label appears.
    expect(screen.queryByText("Pending")).toBeNull();
  });

  it("renders a settled row that carries neither a decision instant nor a decider", () => {
    // `timed_out` and `auto_killed` are nobody's verdict — the sweeper and the
    // daemon close these — so the Decided cell has to survive both fields being
    // absent rather than rendering an empty Time or a blank person.
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [
          gate({ status: "timed_out", updated_at: null, resolved_by: "" }),
        ],
        initialCursor: null,
      }),
    );
    expect(screen.getByText("Timed out")).toBeTruthy();
    // A settled row is not awaiting anything, so the pending arm must not show.
    expect(screen.queryByText(/awaiting review/i)).toBeNull();
  });
});
