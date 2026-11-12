"use client";

import { ConfirmDialog } from "@agentsfleet/design-system";
import type { ApiKeyRow } from "@/lib/api/api_keys";

/** The key + which destructive action is being confirmed. */
export type ConfirmTargetActive = ApiKeyRow & { mode: "revoke" | "delete" };
/** The active target, or null when the dialog is closed. */
export type ConfirmTarget = ConfirmTargetActive | null;

const COPY = {
  revoke: {
    confirmLabel: "Revoke key",
    description: (name: string) => `Tools using “${name}” will lose access immediately. This cannot be undone.`,
  },
  delete: {
    confirmLabel: "Delete",
    description: (name: string) => `This permanently removes the revoked key “${name}”.`,
  },
} as const;

type Props = {
  target: ConfirmTarget;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  onConfirm: (target: ConfirmTargetActive) => Promise<void>;
};

export default function RevokeConfirm({ target, error, onOpenChange, onConfirm }: Props) {
  const mode = target?.mode ?? "revoke";
  const copy = COPY[mode];
  const verb = mode === "revoke" ? "Revoke" : "Delete";
  return (
    <ConfirmDialog
      open={target !== null}
      onOpenChange={onOpenChange}
      title={`${verb} API key?`}
      description={target ? copy.description(target.key_name) : ""}
      confirmLabel={copy.confirmLabel}
      intent="destructive"
      errorMessage={error}
      // Bound to the active target when open; `undefined` when closed so the
      // caller's handler needs no unreachable null guard.
      onConfirm={target ? () => onConfirm(target) : undefined}
    />
  );
}
