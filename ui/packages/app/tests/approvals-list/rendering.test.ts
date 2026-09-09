import { AGENTSFLEET_B, AGENT_A_DISPLAY_NAME, AGENT_B_DISPLAY_NAME, WORKSPACE_ID, gate, render } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import ApprovalsList from "@/app/(dashboard)/w/[workspaceId]/approvals/components/ApprovalsList";

describe("ApprovalsList — EmptyState", () => {
  it("renders the EmptyState when there are no items and no filter", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [],
        initialCursor: null,
      }),
    );
    expect(screen.getByText(/no approvals yet/i)).toBeTruthy();
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
