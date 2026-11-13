import type { ResolveOutcome } from "./approvals";
import type { ApprovalDecision } from "./approvals-types";
import { z } from "zod";
import { APPROVAL_DECISION } from "./approvals-types";

const Receipt = z.object({
  gate_id: z.string().min(1), action_id: z.string().min(1),
  outcome: z.enum(["approved", "denied", "timed_out", "archived"]),
  resolved_at: z.iso.datetime({offset: true}), resolved_by: z.string().min(1),
});
const Outcome = z.discriminatedUnion("kind", [
  z.object({kind: z.literal("resolved"), data: Receipt}),
  z.object({kind: z.literal("already_resolved"), data: Receipt.extend({error_code: z.literal("UZ-APPROVAL-006"), detail: z.string()})}),
]);

/** A successful transport alone cannot confirm which request was decided. */
export function confirmedApprovalResolution(value: unknown, gateId: string, decision: ApprovalDecision): ResolveOutcome | null {
  const parsed = Outcome.safeParse(value);
  if (!parsed.success || parsed.data.data.gate_id !== gateId) return null;
  const expected = decision === APPROVAL_DECISION.APPROVE ? "approved" : "denied";
  if (parsed.data.kind === "resolved" && parsed.data.data.outcome !== expected) return null;
  return parsed.data;
}
