"use client";

import type { ApprovalDecision } from "@/lib/api/approvals-types";
import type { ResolveOutcome } from "@/lib/api/approvals";
import { useLayoutEffect, useRef, useState } from "react";
import { answerApprovalAction, approveApprovalAction, denyApprovalAction } from "@/lib/actions/fleet-approvals";
import { APPROVAL_DECISION } from "@/lib/api/approvals-types";
import { confirmedApprovalResolution } from "@/lib/api/approval-resolution";
import { presentErrorString } from "@/lib/errors";
import { captureProductEvent } from "@/lib/analytics/posthog";
import { EVENTS } from "@/lib/analytics/events";

/** Mount under the gate's scoped identity. The backend arbitrates competing decisions. */
export function useApprovalDecision(workspaceId: string, gateId: string, onResolved: (outcome: ResolveOutcome) => void) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const busy = useRef(false);
  const generation = useRef(0);
  // Revoke ownership at unmount commit, before a late response's microtask.
  useLayoutEffect(() => () => {generation.current += 1;}, []);

  /** `text` is the optional note on approve and deny, and the answer itself on a question. */
  async function resolve(decision: ApprovalDecision, text = "") {
    if (busy.current) return;
    busy.current = true;
    const mine = generation.current;
    setError(null);
    setPending(true);
    try {
      const trimmed = text.trim();
      const answering = decision === APPROVAL_DECISION.ANSWER;
      const result = answering ? await answerApprovalAction(workspaceId, gateId, trimmed)
        : await (decision === APPROVAL_DECISION.APPROVE ? approveApprovalAction : denyApprovalAction)(workspaceId, gateId, trimmed || undefined);
      if (generation.current !== mine) return;
      if (!result.ok) {
        setError(presentErrorString({errorCode: result.errorCode, message: result.error, action: `${decision} this request`}));
        return;
      }
      const outcome = confirmedApprovalResolution(result.data, gateId, decision);
      if (!outcome) throw new Error("Unconfirmed approval decision");
      if (outcome.kind === "resolved") captureProductEvent(EVENTS.approval_resolved, {
        gate_id: gateId, decision, has_reason: !answering && trimmed.length > 0,
      });
      onResolved(outcome);
    } catch {
      if (generation.current === mine) setError("The decision could not be confirmed. Review the request's current status before trying again.");
    } finally {
      if (generation.current === mine) {busy.current = false; setPending(false);}
    }
  }
  return {pending, error, resolve};
}
