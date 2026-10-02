"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { Alert } from "@agentsfleet/design-system";
import type { WaitingInvite } from "@/lib/api/invites";
import { INVITES_PATH } from "@/app/(dashboard)/invites/copy";
import { accountLabel } from "./workspace-groups";

/** The notice's sentence: one invite names its account, several are counted. */
export function inviteNoticeText(waiting: readonly WaitingInvite[]): string {
  const [only] = waiting;
  if (waiting.length === 1 && only) {
    return `You're invited to join ${accountLabel(only.account.owner_name)}.`;
  }
  return `You have ${waiting.length} invites waiting.`;
}

// One line, only while something waits, and not on the page that already
// lists them. It sits inside the padded canvas, where strip styling reads as a
// broken box, so it keeps the standard alert's own look.
export function InviteNotice({ waiting }: { waiting: readonly WaitingInvite[] }) {
  const pathname = usePathname();
  const onInvites = pathname === INVITES_PATH || pathname.startsWith(`${INVITES_PATH}/`);
  if (waiting.length === 0 || onInvites) return null;
  return (
    <Alert variant="info" className="mb-lg" data-testid="invite-notice">
      {inviteNoticeText(waiting)}{" "}
      <Link href={INVITES_PATH} className="underline underline-offset-2">
        Review
      </Link>
    </Alert>
  );
}
