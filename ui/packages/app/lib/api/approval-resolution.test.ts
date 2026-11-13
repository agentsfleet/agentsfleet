import { expect, it } from "vitest";
import { confirmedApprovalResolution } from "./approval-resolution";

const receipt = {gate_id: "gate", action_id: "action", outcome: "approved", resolved_at: "2026-11-12T12:00:00Z", resolved_by: "reviewer"};

it("accepts the requested decision only for the matching gate", () => {
  const value = {kind: "resolved", data: receipt};
  expect(confirmedApprovalResolution(value, "gate", "approve")).toEqual(value);
  expect(confirmedApprovalResolution(value, "other", "approve")).toBeNull();
  expect(confirmedApprovalResolution(value, "gate", "deny")).toBeNull();
  expect(confirmedApprovalResolution({...value, data: {...receipt, outcome: "denied"}}, "gate", "deny")).not.toBeNull();
});

it.each(["approved", "denied", "timed_out", "archived"])("accepts a recorded competing terminal outcome: %s", outcome => {
  const value = {kind: "already_resolved", data: {...receipt, outcome, error_code: "UZ-APPROVAL-006", detail: "Already decided"}};
  expect(confirmedApprovalResolution(value, "gate", "deny")).toEqual(value);
});

it.each([null, {}, {kind: "resolved", data: {...receipt, outcome: "pending"}},
  {kind: "already_resolved", data: receipt}, {kind: "already_resolved", data: {...receipt, error_code: "UZ-OTHER", detail: "Conflict"}},
  {kind: "resolved", data: {...receipt, resolved_at: "invalid"}},
])("refuses missing, malformed or nonterminal receipts %#", value => {
  expect(confirmedApprovalResolution(value, "gate", "approve")).toBeNull();
});
