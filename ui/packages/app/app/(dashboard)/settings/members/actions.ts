"use server";

import { withToken, type ActionResult } from "@/lib/actions/with-token";
import { createInvite, listInvites, revokeInvite, type InviteSummary } from "@/lib/api/invites";
import { listMembers, removeMember, type MemberSummary } from "@/lib/api/tenant-members";

export type Team = { members: MemberSummary[]; invites: InviteSummary[] };

export async function loadTeamAction(): Promise<ActionResult<Team>> {
  return withToken(async (token) => {
    const [members, invites] = await Promise.all([listMembers(token), listInvites(token)]);
    return { members, invites };
  });
}

export async function createInviteAction(email: string): Promise<ActionResult<InviteSummary>> {
  return withToken((token) => createInvite(token, email));
}

export async function revokeInviteAction(inviteId: string): Promise<ActionResult<void>> {
  return withToken((token) => revokeInvite(token, inviteId));
}

export async function removeMemberAction(userId: string): Promise<ActionResult<void>> {
  return withToken((token) => removeMember(token, userId));
}
