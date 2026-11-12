"use client";

import type { ApprovalDecision } from "@/lib/api/approvals-types";
import type { ResolveOutcome } from "@/lib/api/approvals";
import { useEffect, useRef, useState } from "react";
import { approveApprovalAction, denyApprovalAction } from "@/lib/actions/fleet-approvals";
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
  useEffect(() => () => {generation.current += 1;}, []);

  async function resolve(decision: ApprovalDecision, reason = "") {
    if (busy.current) return;
    busy.current = true;
    const mine = generation.current;
    setError(null);
    setPending(true);
    try {
      const trimmed = reason.trim();
      const action = decision === APPROVAL_DECISION.APPROVE ? approveApprovalAction : denyApprovalAction;
      const result = await action(workspaceId, gateId, trimmed || undefined);
      if (generation.current !== mine) return;
      if (!result.ok) {
        setError(presentErrorString({errorCode: result.errorCode, message: result.error, action: `${decision} this request`}));
        return;
      }
      const outcome = confirmedApprovalResolution(result.data, gateId, decision);
      if (!outcome) throw new Error("Unconfirmed approval decision");
      if (outcome.kind === "resolved") captureProductEvent(EVENTS.approval_resolved, {
        gate_id: gateId, decision, has_reason: trimmed.length > 0,
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
