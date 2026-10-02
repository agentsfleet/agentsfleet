"use server";

import { revalidatePath } from "next/cache";
import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { acceptInvite, type AcceptedInvite } from "@/lib/api/invites";

// Accepting changes what the shell shows on every page: the switcher gains the
// account's workspaces and the pending-invite notice may go. Both live in the
// dashboard layout, which a client navigation keeps, so the layout revalidates.
export async function acceptInviteAction(inviteId: string): Promise<ActionResult<AcceptedInvite>> {
  const result = await withToken((token) => acceptInvite(token, inviteId));
  if (result.ok) revalidatePath("/", "layout");
  return result;
}
