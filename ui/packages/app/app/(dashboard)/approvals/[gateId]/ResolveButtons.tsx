"use client";

import { useRouter } from "next/navigation";
import { useState, useTransition } from "react";
import { Button, Label, Textarea } from "@usezombie/design-system";

import { useClientToken } from "@/lib/auth/client";
import { approveApproval, denyApproval } from "@/lib/api/approvals";

type Props = {
  workspaceId: string;
  gateId: string;
};

export default function ResolveButtons({ workspaceId, gateId }: Props) {
  const router = useRouter();
  const { getToken } = useClientToken();
  const [reason, setReason] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();

  function resolve(decision: "approve" | "deny") {
    setError(null);
    startTransition(async () => {
      const token = await getToken();
      if (!token) {
        setError("Not authenticated");
        return;
      }
      const fn = decision === "approve" ? approveApproval : denyApproval;
      try {
        const outcome = await fn(workspaceId, gateId, token, reason || undefined);
        if (outcome.kind === "already_resolved") {
          // Refresh the page so the operator sees the terminal-state view
          // with the canonical resolver attribution.
          router.refresh();
          return;
        }
        // Success — bounce back to the inbox so they see the fresh queue.
        router.push("/approvals");
        router.refresh();
      } catch (e) {
        setError((e as Error).message ?? "Resolve failed");
      }
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
        <Button onClick={() => resolve("approve")} disabled={pending} aria-busy={pending}>
          Approve
        </Button>
        <Button variant="destructive" onClick={() => resolve("deny")} disabled={pending} aria-busy={pending}>
          Deny
        </Button>
      </div>
      {error ? (
        <div role="alert" className="text-sm text-destructive">
          {error}
        </div>
      ) : null}
    </div>
  );
}
