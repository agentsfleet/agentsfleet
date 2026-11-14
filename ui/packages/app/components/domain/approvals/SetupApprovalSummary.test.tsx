import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { EMPTY_DRAFT_VIEW } from "@/lib/api/draft-state";
import { FLEET, WORKSPACE } from "@/tests/helpers/draft-fixtures";
import { SetupApprovalSummary } from "./SetupApprovalSummary";

const mocks = vi.hoisted(() => ({list: vi.fn(), approve: vi.fn(), deny: vi.fn()}));
vi.mock("@/lib/actions/fleet-approvals", () => ({listApprovalsAction: mocks.list, approveApprovalAction: mocks.approve, denyApprovalAction: mocks.deny}));
const GATE = {gate_id: "setup-gate", workspace_id: WORKSPACE, fleet_id: FLEET.id, status: "pending",
  proposed_action: "Allow GitHub access", tool_name: "github", action_name: "access", blast_radius: "Selected repositories"};
beforeEach(() => {vi.clearAllMocks(); mocks.list.mockResolvedValue({ok: true, data: {items: [GATE], next_cursor: null}});});
afterEach(cleanup);

it("offers the saved fleet's connector decision before Finish setup without approving automatically", async () => {
  const view = render(<SetupApprovalSummary subject="operator" workspace={WORKSPACE} view={EMPTY_DRAFT_VIEW} />);
  expect(mocks.list).not.toHaveBeenCalled();
  view.rerender(<SetupApprovalSummary subject="operator" workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, fleet: FLEET}} />);
  expect(await screen.findByRole("button", {name: "Approve"})).toBeTruthy();
  expect(mocks.list).toHaveBeenCalledWith(WORKSPACE, {fleetId: FLEET.id, status: "pending", limit: 1});
  expect(mocks.approve).not.toHaveBeenCalled();
  expect(screen.queryByRole("link")).toBeNull();
});

it.each(["signed-out", "inaccessible"])("removes decisions and ignores a late read after becoming %s", async state => {
  const pending = Promise.withResolvers<unknown>(); mocks.list.mockReturnValue(pending.promise);
  const view = render(<SetupApprovalSummary subject="operator" workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, fleet: FLEET}} />);
  view.rerender(<SetupApprovalSummary subject={state === "signed-out" ? undefined : "operator"} workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, fleet: FLEET, inaccessible: state === "inaccessible"}} />);
  await act(async () => pending.resolve({ok: true, data: {items: [GATE], next_cursor: null}}));
  expect(screen.queryByRole("button", {name: "Approve"})).toBeNull();
  expect(mocks.approve).not.toHaveBeenCalled();
});

it("rejects a saved gate from another workspace", async () => {
  render(<SetupApprovalSummary subject="operator" workspace="another-workspace" view={{...EMPTY_DRAFT_VIEW, fleet: FLEET, activities: []}} />);
  expect(await screen.findByText("Request details unavailable.")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Approve"})).toBeNull();
});

it("refreshes saved setup readiness only after a confirmed decision", async () => {
  const onResolved = vi.fn();
  mocks.approve.mockResolvedValueOnce({ok: false, status: 403, error: "Forbidden"}).mockResolvedValueOnce({ok: true, data: {kind: "resolved", data: {
    gate_id: GATE.gate_id, action_id: "github-access", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "operator",
  }}});
  render(<SetupApprovalSummary subject="operator" workspace={WORKSPACE} view={{...EMPTY_DRAFT_VIEW, fleet: FLEET}} onResolved={onResolved} />);
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  await screen.findByRole("alert");
  expect(onResolved).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Approve"}));
  await screen.findByText("Request approved.");
  expect(onResolved).toHaveBeenCalledTimes(1);
});
