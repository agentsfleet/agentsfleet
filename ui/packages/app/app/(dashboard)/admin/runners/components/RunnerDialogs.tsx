"use client";

import { ConfirmDialog } from "@agentsfleet/design-system";
import type { RunnerAdminAction, RunnerListItem } from "@/lib/api/runners";

const CONFIRM_LABEL = "Confirm";

// The confirm copy the dialog actually renders. Delete carries the same copy but
// no `action` — it is a DELETE verb, not one of the three PATCH admin actions —
// so the copy shape is factored out rather than widening RunnerAdminAction.
export type RunnerConfirmCopy = {
  runner: RunnerListItem;
  label: string;
  title: string;
  description: string;
  intent: "default" | "destructive";
  errorAction: string;
};

export type RunnerActionConfirmTarget = (RunnerConfirmCopy & { action: RunnerAdminAction }) | null;
export type RunnerDeleteConfirmTarget = RunnerConfirmCopy | null;

// Generic over the target so both callers keep their exact shape: the PATCH
// caller's handler needs `action`, the delete caller's does not. A non-generic
// RunnerConfirmCopy parameter would reject the PATCH handler outright.
export function RunnerActionConfirm<T extends RunnerConfirmCopy>({
  target,
  error,
  onOpenChange,
  onConfirm,
}: {
  target: T | null;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: (target: T) => void;
}) {
  return (
    <ConfirmDialog
      open={target !== null}
      onOpenChange={onOpenChange}
      title={target?.title ?? ""}
      description={target?.description ?? ""}
      confirmLabel={target?.label ?? CONFIRM_LABEL}
      intent={target?.intent ?? "default"}
      errorMessage={error}
      onConfirm={target ? () => onConfirm(target) : undefined}
    />
  );
}
