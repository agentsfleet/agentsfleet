import type { ApprovalsListResponse, ResolveOutcome } from "@/lib/api/approvals";
import type { ActionResult } from "@/lib/actions/with-token";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { FleetApprovalSummary } from "./FleetApprovalSummary";
import { WorkApprovals } from "./WorkContextSections";

const mocks = vi.hoisted(() => ({list: vi.fn(), deny: vi.fn(), approve: vi.fn(), refresh: vi.fn(), capture: vi.fn()}));
vi.mock("@/lib/actions/fleet-approvals", () => ({listApprovalsAction: mocks.list, denyApprovalAction: mocks.deny, approveApprovalAction: mocks.approve}));
vi.mock("next/navigation", () => ({useRouter: () => ({refresh: mocks.refresh})}));
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: mocks.capture}));
const PROPS = {workspaceId: "workspace", fleetId: "fleet", revision: 1, count: 1, href: "/w/workspace/fleets/fleet#fleet-approvals"};
const GATE = {gate_id: "gate-one", workspace_id: PROPS.workspaceId, fleet_id: PROPS.fleetId, status: "pending",
  proposed_action: "Publish runner image", tool_name: "registry", action_name: "publish", blast_radius: "Production registry · Release tag"};
const page = (items: Array<typeof GATE & {detail?: string; evidence?: Record<string, unknown>; expires_at?: string}> = [GATE]) => ({ok: true, data: {items, next_cursor: null}});
beforeEach(() => {vi.clearAllMocks(); mocks.list.mockReset(); mocks.deny.mockReset(); mocks.approve.mockReset(); mocks.list.mockResolvedValue(page());});
afterEach(cleanup);

it("reads only this fleet's pending gate and exposes decisions without page navigation", async () => {
  render(<FleetApprovalSummary {...PROPS} />);
  expect(await screen.findByText(GATE.proposed_action)).toBeTruthy();
  expect(mocks.list).toHaveBeenCalledWith("workspace", {fleetId: "fleet", status: "pending", limit: 1});
  expect(screen.queryByRole("link")).toBeNull();
  expect(screen.getByText(GATE.blast_radius)).toBeTruthy();
  expect(screen.getByRole("button", {name: "Approve"})).toBeTruthy();
});

it("denies directly, blocks repeat submissions, then shows the confirmed outcome in place", async () => {
  const decision = Promise.withResolvers<ActionResult<ResolveOutcome>>();
  mocks.deny.mockReturnValue(decision.promise);
  render(<FleetApprovalSummary {...PROPS} />);
  const deny = await screen.findByRole("button", {name: "Deny"});
  fireEvent.click(deny); fireEvent.click(deny);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  expect(mocks.deny).toHaveBeenCalledTimes(1);
  expect(mocks.deny).toHaveBeenCalledWith("workspace", "gate-one", undefined);
  await act(async () => decision.resolve({ok: true, data: {kind: "resolved", data: {
    gate_id: "gate-one", action_id: "publish", outcome: "denied", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "reviewer",
  }}}));
  expect(await screen.findByText(/Request denied/)).toBeTruthy();
  expect(screen.getByRole("region", {name: "Approval decision"})).toBeTruthy();
  expect(screen.queryByRole("region", {name: "Needs your approval"})).toBeNull();
  expect(screen.queryByText("1", {selector: "h2 span"})).toBeNull();
  expect(screen.queryByRole("button", {name: "Deny"})).toBeNull();
  expect(screen.getByTitle("reviewer")).toBeTruthy();
  expect(screen.queryByRole("link")).toBeNull();
});

it("shows a competing approval without reporting that Deny succeeded", async () => {
  mocks.deny.mockResolvedValue({ok: true, data: {kind: "already_resolved", data: {gate_id: "gate-one", action_id: "publish", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "reviewer", error_code: "UZ-APPROVAL-006", detail: "Decided elsewhere"}}});
  render(<FleetApprovalSummary {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Deny"}));
  expect(await screen.findByText(/Request approved/)).toBeTruthy();
  expect(screen.getByRole("region", {name: "Approval decision"})).toBeTruthy();
  expect(mocks.capture).not.toHaveBeenCalled();
});

it("retains an uncertain decision error and permits status review", async () => {
  mocks.deny.mockRejectedValue(new Error("offline"));
  render(<FleetApprovalSummary {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Deny"}));
  expect(await screen.findAllByText(/decision could not be confirmed/)).not.toHaveLength(0);
  expect(mocks.refresh).not.toHaveBeenCalled();
});

it.each([
  {...GATE, workspace_id: "another-workspace"}, {...GATE, fleet_id: "another-fleet"}, {...GATE, status: "approved"}, {bad: true},
])("never exposes actions for mismatched or malformed gate data %#", async gate => {
  mocks.list.mockResolvedValue({ok: true, data: {items: [gate], next_cursor: null}});
  render(<FleetApprovalSummary {...PROPS} />);
  expect(await screen.findByText("Request details unavailable.")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Deny"})).toBeNull();
});

it("keeps failed reads distinct from an empty inbox and retries explicitly", async () => {
  mocks.list.mockRejectedValueOnce(new Error("offline")).mockResolvedValueOnce(page([]));
  render(<FleetApprovalSummary {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Try again"}));
  await waitFor(() => expect(screen.queryByRole("region", {name: "Needs your approval"})).toBeNull());
  expect(mocks.list).toHaveBeenCalledTimes(2);
});

it("clears actions on authorization refusal and ignores a superseded read", async () => {
  const old = Promise.withResolvers<ActionResult<ApprovalsListResponse>>();
  mocks.list.mockReturnValueOnce(old.promise).mockResolvedValueOnce({ok: false, status: 403, error: "Forbidden"});
  const view = render(<FleetApprovalSummary {...PROPS} />);
  view.rerender(<FleetApprovalSummary {...PROPS} revision={2} />);
  expect(await screen.findByText("Request details unavailable.")).toBeTruthy();
  await act(async () => old.reject(new Error("late response")));
  expect(screen.queryByRole("button", {name: "Deny"})).toBeNull();
  await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));
});

it("uses the recorded tool action when a proposed title and blast radius are absent", async () => {
  mocks.list.mockResolvedValue(page([{...GATE, proposed_action: "", blast_radius: ""}]));
  render(<FleetApprovalSummary {...PROPS} />);
  expect(await screen.findByText("registry:publish")).toBeTruthy();
  expect(screen.queryByText(GATE.blast_radius)).toBeNull();
});


it.each([{}, {kind: "resolved", data: {gate_id: "wrong-gate", outcome: "denied"}}, {kind: "already_resolved", data: {outcome: "pending"}}])("does not confirm a malformed or unrelated decision %#", async data => {
  mocks.deny.mockResolvedValue({ok: true, data});
  render(<FleetApprovalSummary {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Deny"}));
  expect(await screen.findAllByText(/decision could not be confirmed/)).not.toHaveLength(0);
  expect(screen.getByRole("button", {name: "Check status"})).toBeTruthy();
  expect(mocks.capture).not.toHaveBeenCalled();
  expect(mocks.refresh).not.toHaveBeenCalled();
});

it("replaces a resolved request after a gate hint even when one request still remains", async () => {
  render(<FleetApprovalSummary {...PROPS} />).rerender(<FleetApprovalSummary {...PROPS} revision={2} />);
  expect(await screen.findByText(GATE.proposed_action)).toBeTruthy();
  const stale = Promise.withResolvers<ActionResult<ApprovalsListResponse>>();
  mocks.list.mockReturnValueOnce(stale.promise).mockResolvedValueOnce(page([{...GATE, gate_id: "gate-two", proposed_action: "Publish release notes"}]));
  cleanup();
  const view = render(<FleetApprovalSummary {...PROPS} />);
  view.rerender(<FleetApprovalSummary {...PROPS} revision={2} />);
  expect(await screen.findByText("Publish release notes")).toBeTruthy();
  await act(async () => stale.resolve(page() as ActionResult<ApprovalsListResponse>));
  expect(screen.queryByText(GATE.proposed_action)).toBeNull();
  expect(screen.getByRole("button", {name: "Approve"}).closest("[data-approval-gate]")?.getAttribute("data-approval-gate")).toBe("gate-two");
});


it("discovers the first gate even when the rendered pending count was zero", async () => {
  mocks.list.mockResolvedValueOnce(page([])).mockResolvedValueOnce(page());
  const view = render(<FleetApprovalSummary {...PROPS} count={0} />);
  await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(1));
  expect(screen.queryByRole("region", {name: "Needs your approval"})).toBeNull();
  view.rerender(<FleetApprovalSummary {...PROPS} count={0} revision={2} />);
  expect(await screen.findByText(GATE.proposed_action)).toBeTruthy();
  expect(screen.getByRole("button", {name: "Deny"})).toBeTruthy();
});

it("keeps an unavailable inbox distinct from confirmed zero pending approvals", async () => {
  mocks.list.mockResolvedValue({ok: false, status: 503, error: "Unavailable"});
  render(<FleetApprovalSummary {...PROPS} count={0} />);
  expect(await screen.findByRole("region", {name: "Approvals"})).toBeTruthy();
  expect(screen.getByRole("button", {name: "Try again"})).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Deny"})).toBeNull();
});

it("refreshes approval details when the pending count changes without a stream hint", async () => {
  mocks.list.mockResolvedValueOnce(page([])).mockResolvedValueOnce(page());
  const scope = {...PROPS, subject: "account"};
  const view = render(<WorkApprovals scope={scope} approvals={{count: 0, href: PROPS.href}} />);
  await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(1));
  expect(screen.queryByRole("button", {name: "Deny"})).toBeNull();
  view.rerender(<WorkApprovals scope={scope} approvals={{count: 1, href: PROPS.href}} />);
  expect(await screen.findByText(GATE.proposed_action)).toBeTruthy();
  expect(mocks.list).toHaveBeenCalledTimes(2);
});

it("opens the fleet when a scoped reader is unavailable", () => {
  render(<WorkApprovals approvals={{count: 1, href: PROPS.href}} />);
  expect(screen.getByRole("link", {name: "Open fleet with 1 pending approval"}).getAttribute("href")).toBe(PROPS.href);
  expect(mocks.list).not.toHaveBeenCalled();
});

it("approves the exact request directly and reads the next pending request on demand", async () => {
  mocks.approve.mockResolvedValue({ok: true, data: {kind: "resolved", data: {
    gate_id: "gate-one", action_id: "publish", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "reviewer",
  }}});
  mocks.list.mockResolvedValueOnce(page()).mockResolvedValueOnce(page([{...GATE, gate_id: "gate-two", proposed_action: "Publish release notes"}]));
  render(<FleetApprovalSummary {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  expect(mocks.approve).toHaveBeenCalledWith("workspace", "gate-one", undefined);
  expect(await screen.findByText("Request approved.")).toBeTruthy();
  expect(screen.queryByRole("button", {name: "Approve"})).toBeNull();
  expect(mocks.deny).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", {name: "Check remaining requests"}));
  expect(await screen.findByText("Publish release notes")).toBeTruthy();
  expect(screen.getByRole("button", {name: "Approve"})).toBeTruthy();
});

it("keeps evidence and a reason in the conversation and retains authorization errors", async () => {
  mocks.list.mockResolvedValue(page([{...GATE, detail: "Only the release tag", evidence: {image: "runner:release"}, expires_at: "2026-11-12T12:00:00Z"}]));
  mocks.approve.mockResolvedValue({ok: false, status: 403, error: "Not permitted", errorCode: "UZ-AUTH-022"});
  render(<FleetApprovalSummary {...PROPS} />);
  await screen.findByText(GATE.proposed_action);
  fireEvent.click(screen.getByRole("button", {name: "View exact action"}));
  expect(screen.getByRole("dialog", {name: "Exact action"})).toBeTruthy();
  expect(screen.getByText("Only the release tag")).toBeTruthy();
  expect(screen.getByText(/runner:release/)).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Reason (optional)"), {target: {value: "  release reviewed  "}});
  fireEvent.click(screen.getByRole("button", {name: "Done"}));
  fireEvent.click(screen.getByRole("button", {name: "Approve"}));
  await screen.findByRole("alert");
  expect(mocks.approve).toHaveBeenCalledWith("workspace", "gate-one", "release reviewed");
  expect(screen.queryByText("Request approved.")).toBeNull();
  expect(screen.getByRole("button", {name: "Check status"})).toBeTruthy();
  expect(mocks.capture).not.toHaveBeenCalled();
});

it("retains a pending decision and its receipt through a count and stream revision update", async () => {
  const receipt = Promise.withResolvers<ActionResult<ResolveOutcome>>();
  mocks.approve.mockReturnValue(receipt.promise);
  mocks.list.mockResolvedValueOnce(page()).mockResolvedValueOnce(page([]));
  const scope = {...PROPS, subject: "operator"};
  const view = render(<WorkApprovals scope={scope} approvals={{count: 1, href: PROPS.href}} />);
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  view.rerender(<WorkApprovals scope={{...scope, revision: 2}} approvals={{count: 0, href: PROPS.href}} />);
  await waitFor(() => expect(mocks.list).toHaveBeenCalledTimes(2));
  expect(screen.getByRole("button", {name: "Approve"})).toHaveProperty("disabled", true);
  await act(async () => receipt.resolve({ok: true, data: {kind: "resolved", data: {
    gate_id: "gate-one", action_id: "publish", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "operator",
  }}}));
  expect(await screen.findByText("Request approved.")).toBeTruthy();
  expect(mocks.approve).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("button", {name: "Approve"})).toBeNull();
});

it("drops a pending decision and its late receipt when a refreshed read revokes access", async () => {
  const receipt = Promise.withResolvers<ActionResult<ResolveOutcome>>();
  mocks.approve.mockReturnValue(receipt.promise);
  mocks.list.mockResolvedValueOnce(page()).mockResolvedValueOnce({ok: false, status: 403, error: "Forbidden"});
  const view = render(<FleetApprovalSummary {...PROPS} />);
  fireEvent.click(await screen.findByRole("button", {name: "Approve"}));
  view.rerender(<FleetApprovalSummary {...PROPS} revision={2} />);
  await screen.findByText("Request details unavailable.");
  await act(async () => receipt.resolve({ok: true, data: {kind: "resolved", data: {
    gate_id: "gate-one", action_id: "publish", outcome: "approved", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "operator",
  }}}));
  expect(screen.queryByText("Request approved.")).toBeNull();
  expect(screen.queryByRole("button", {name: "Approve"})).toBeNull();
  expect(mocks.capture).not.toHaveBeenCalled();
});
