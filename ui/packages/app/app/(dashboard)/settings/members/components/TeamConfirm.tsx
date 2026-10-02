"use client";

import { useState } from "react";
import { ConfirmDialog } from "@agentsfleet/design-system";
import type { InviteSummary } from "@/lib/api/invites";
import type { MemberSummary } from "@/lib/api/tenant-members";

export const CONFIRM_KIND = {
  revoke: "revoke",
  remove: "remove",
} as const;

/** What the owner is about to undo: an invite, or a person's membership. */
export type ConfirmTargetActive =
  | { kind: typeof CONFIRM_KIND.revoke; invite: InviteSummary }
  | { kind: typeof CONFIRM_KIND.remove; member: MemberSummary };
/** The active target, or null while the dialog is closed. */
export type ConfirmTarget = ConfirmTargetActive | null;

export function memberName(member: MemberSummary): string {
  return member.display_name ?? member.email;
}

function copyFor(target: ConfirmTargetActive): { title: string; description: string; confirmLabel: string } {
  if (target.kind === CONFIRM_KIND.revoke) {
    return {
      title: `Revoke the invite for ${target.invite.email}?`,
      description: "Its link stops working. You can invite the same address again.",
      confirmLabel: "Revoke",
    };
  }
  return {
    title: `Remove ${memberName(target.member)}?`,
    description: "They lose access to every workspace in your account within 15 seconds, including any page they have open.",
    confirmLabel: "Remove",
  };
}

type Props = {
  target: ConfirmTarget;
  error: string | null;
  onOpenChange: (open: boolean) => void;
  /** Settles once the request has answered; the dialog's buttons hold until then. */
  onConfirm: (target: ConfirmTargetActive) => Promise<void>;
};

export function TeamConfirm({ target, error, onOpenChange, onConfirm }: Props) {
  // The dialog animates out after `target` clears, so it keeps the last
  // target's words rather than closing on a blank title and a default button.
  const [last, setLast] = useState<ConfirmTargetActive | null>(target);
  if (target !== null && target !== last) setLast(target);
  const shown = target ?? last;
  const copy = shown ? copyFor(shown) : null;
  return (
    <ConfirmDialog
      open={target !== null}
      onOpenChange={onOpenChange}
      title={copy?.title ?? ""}
      description={copy?.description}
      confirmLabel={copy?.confirmLabel}
      intent="destructive"
      errorMessage={error}
      // Bound only while a target is open, so the handler needs no null guard.
      onConfirm={target ? () => onConfirm(target) : undefined}
    />
  );
}
