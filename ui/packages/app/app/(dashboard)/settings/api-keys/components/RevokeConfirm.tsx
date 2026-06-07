"use client";

import { ConfirmDialog } from "@usezombie/design-system";
import type { ApiKeyRow } from "@/lib/api/api_keys";

/** The key + which destructive action is being confirmed. */
export type ConfirmTargetActive = ApiKeyRow & { mode: "revoke" | "delete" };
/** The active target, or null when the dialog is closed. */
export type ConfirmTarget = ConfirmTargetActive | null;

const COPY = {
  revoke: {
    confirmLabel: "Revoke",
    description:
      "Any service still using this key starts getting 401s immediately. This cannot be undone — a revoked key cannot be reactivated, only deleted.",
  },
  delete: {
    confirmLabel: "Delete",
    description: "This permanently removes the revoked key's record. Only already-revoked keys can be deleted.",
  },
} as const;

type Props = {
  target: ConfirmTarget;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: (target: ConfirmTargetActive) => void;
};

export default function RevokeConfirm({ target, error, onOpenChange, onConfirm }: Props) {
  const mode = target?.mode ?? "revoke";
  const copy = COPY[mode];
  const verb = mode === "revoke" ? "Revoke" : "Delete";
  return (
    <ConfirmDialog
      open={target !== null}
      onOpenChange={onOpenChange}
      title={`${verb} API key "${target?.key_name ?? ""}"?`}
      description={copy.description}
      confirmLabel={copy.confirmLabel}
      intent="destructive"
      errorMessage={error}
      // Bound to the active target when open; `undefined` when closed so the
      // caller's handler needs no unreachable null guard.
      onConfirm={target ? () => onConfirm(target) : undefined}
    />
  );
}
