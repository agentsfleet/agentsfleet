import { AGENTSFLEET_B, AGENT_A_DISPLAY_NAME, AGENT_B_DISPLAY_NAME, WORKSPACE_ID, gate } from "./harness";
import React from "react";
import { describe, expect, it } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
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
    expect(screen.getByText(/no pending approvals/i)).toBeTruthy();
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
    expect(screen.getByRole("button", { name: /^approve$/i })).toBeTruthy();
    expect(screen.getByRole("button", { name: /^deny$/i })).toBeTruthy();
    expect(screen.getByRole("link", { name: /details/i })).toBeTruthy();
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

describe("ApprovalsList — client-side filter", () => {
  it("hides rows that don't match the filter input", () => {
    const items = [
      gate({ gate_id: "01999999-1111-7000-8000-000000000001", proposed_action: "Open PR titled wire" }),
      gate({
        gate_id: "01999999-1111-7000-8000-000000000002",
        proposed_action: "Drop production database",
        fleet_name: "approvals-b",
        action_id: "a2",
      }),
    ];
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: items,
        initialCursor: null,
      }),
    );
    fireEvent.change(screen.getByLabelText(/filter approvals/i), {
      target: { value: "wire" },
    });
    expect(screen.getByText(/Open PR titled wire/i)).toBeTruthy();
    expect(screen.queryByText(/Drop production database/i)).toBeNull();
  });

  it("matches the fleet identity shown on each approval", () => {
    render(
      React.createElement(ApprovalsList, {
        workspaceId: WORKSPACE_ID,
        initialItems: [gate({ proposed_action: "Restart the worker", fleet_name: "hidden-slug" })],
        initialCursor: null,
      }),
    );

    fireEvent.change(screen.getByLabelText(/filter approvals/i), {
      target: { value: AGENT_A_DISPLAY_NAME },
    });

    expect(screen.getByText("Restart the worker")).toBeTruthy();
  });
});
