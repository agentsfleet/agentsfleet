"use client";

import { useRouter } from "next/navigation";
import { useState, useTransition } from "react";
import { Alert, Button, Label, Textarea } from "@usezombie/design-system";

import { approveApprovalAction, denyApprovalAction } from "../actions";
import { APPROVAL_DECISION, type ApprovalDecision } from "@/lib/api/approvals";
import { presentErrorString } from "@/lib/errors";
import { EVENTS } from "@/lib/analytics/events";
import { captureProductEvent } from "@/lib/analytics/posthog";

type Props = {
  workspaceId: string;
  gateId: string;
};

export default function ResolveButtons({ workspaceId, gateId }: Props) {
  const router = useRouter();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function resolve(decision: ApprovalDecision) {
    setError(null);
    startTransition(async () => {
      const isApprove = decision === APPROVAL_DECISION.APPROVE;
      const action = isApprove ? approveApprovalAction : denyApprovalAction;
      // One trimmed value feeds both the action and has_reason, so the
      // analytics flag cannot disagree with what the server stored.
      const trimmedReason = reason.trim();
      const result = await action(workspaceId, gateId, trimmedReason || undefined);
      if (!result.ok) {
        setError(
          presentErrorString({
            errorCode: result.errorCode,
            message: result.error,
            action: isApprove ? "approve this request" : "deny this request",
          }),
        );
        return;
      }
      if (result.data.kind === "already_resolved") {
        // Refresh the page so the operator sees the terminal-state view
        // with the canonical resolver attribution.
        router.refresh();
        return;
      }
      // Success — bounce back to the inbox so they see the fresh queue.
      captureProductEvent(EVENTS.approval_resolved, {
        gate_id: gateId,
        decision,
        has_reason: trimmedReason.length > 0,
      });
      router.push("/approvals");
      router.refresh();
    });
  }

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-2">
        <Label htmlFor="reason">Reason (optional)</Label>
        <Textarea
          id="reason"
          value={reason}
          onChange={(e) => setReason(e.currentTarget.value)}
          rows={3}
          placeholder="Why are you approving/denying this action?"
          maxLength={4096}
        />
      </div>
      <div className="flex gap-2">
        <Button onClick={() => resolve(APPROVAL_DECISION.APPROVE)} disabled={pending} aria-busy={pending}>
          Approve
        </Button>
        <Button
          variant="destructive"
          onClick={() => resolve(APPROVAL_DECISION.DENY)}
          disabled={pending}
          aria-busy={pending}
        >
          Deny
        </Button>
      </div>
      {error ? (
        <Alert variant="destructive">{error}</Alert>
      ) : null}
    </div>
  );
}
