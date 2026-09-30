"use client";

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
  onConfirm: (target: ConfirmTargetActive) => void;
};

export function TeamConfirm({ target, error, onOpenChange, onConfirm }: Props) {
  const copy = target ? copyFor(target) : null;
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
