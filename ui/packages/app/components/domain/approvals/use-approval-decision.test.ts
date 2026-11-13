import type { ActionResult } from "@/lib/actions/with-token";
import type { ResolveOutcome } from "@/lib/api/approvals";
import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { APPROVAL_DECISION } from "@/lib/api/approvals-types";
import { useApprovalDecision } from "./use-approval-decision";

const mocks = vi.hoisted(() => ({approve: vi.fn(), deny: vi.fn(), capture: vi.fn()}));
vi.mock("@/lib/actions/fleet-approvals", () => ({approveApprovalAction: mocks.approve, denyApprovalAction: mocks.deny}));
vi.mock("@/lib/analytics/posthog", () => ({captureProductEvent: mocks.capture}));
const OUTCOME: ResolveOutcome = {kind: "resolved", data: {
  gate_id: "gate", action_id: "publish", outcome: "denied", resolved_at: "2026-11-12T10:00:00Z", resolved_by: "reviewer",
}};

beforeEach(() => vi.resetAllMocks());
afterEach(cleanup);

it("submits one decision while pending and allows retry after an explicit refusal", async () => {
  const request = Promise.withResolvers<ActionResult<ResolveOutcome>>();
  mocks.deny.mockReturnValueOnce(request.promise).mockResolvedValueOnce({ok: true, data: OUTCOME});
  const onResolved = vi.fn();
  const {result} = renderHook(() => useApprovalDecision("workspace", "gate", onResolved));
  let submitted: Promise<void> | undefined;
  act(() => {submitted = result.current.resolve(APPROVAL_DECISION.DENY, "  Incorrect image  ");});
  await act(() => result.current.resolve(APPROVAL_DECISION.APPROVE));
  expect(result.current.pending).toBe(true);
  expect(mocks.deny).toHaveBeenCalledWith("workspace", "gate", "Incorrect image");
  expect(mocks.approve).not.toHaveBeenCalled();
  await act(async () => {request.resolve({ok: false, status: 403, error: "Not allowed"}); await submitted;});
  expect(result.current.pending).toBe(false);
  expect(result.current.error).toBeTruthy();
  expect(onResolved).not.toHaveBeenCalled();
  await act(() => result.current.resolve(APPROVAL_DECISION.DENY));
  expect(onResolved).toHaveBeenCalledWith(OUTCOME);
  expect(result.current.error).toBeNull();
  expect(mocks.deny).toHaveBeenCalledTimes(2);
});

it.each(["fulfilled", "rejected"])("ignores a %s decision response after its scope unmounts", async completion => {
  const request = Promise.withResolvers<ActionResult<ResolveOutcome>>();
  mocks.deny.mockReturnValue(request.promise);
  const onResolved = vi.fn();
  const {result, unmount} = renderHook(() => useApprovalDecision("workspace", "gate", onResolved));
  let submitted: Promise<void> | undefined;
  act(() => {submitted = result.current.resolve(APPROVAL_DECISION.DENY);});
  unmount();
  await act(async () => {
    if (completion === "fulfilled") request.resolve({ok: true, data: OUTCOME});
    else request.reject(new Error("Response lost"));
    await submitted;
  });
  expect(onResolved).not.toHaveBeenCalled();
  expect(mocks.capture).not.toHaveBeenCalled();
});
