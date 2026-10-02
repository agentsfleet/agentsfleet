import { redirect } from "next/navigation";
import { requireCredential } from "@/lib/auth/credential";
import { SIGN_IN_PATH } from "@/lib/auth/sign-in-redirect";
import { ApiError, HTTP_STATUS_UNAUTHORIZED } from "@/lib/api/errors";
import { listInvites, type InviteSummary } from "@/lib/api/invites";
import { listMembers, type MemberSummary } from "@/lib/api/tenant-members";
import { MembersView } from "./components/MembersView";

export const dynamic = "force-dynamic";

// The caller's own account: every person owns the one signup made them, so
// this page is always theirs to manage. Both lists are one page each.
export default async function MembersPage() {
  const [members, invites] = await loadTeam(await requireCredential());
  return <MembersView initialMembers={members} initialInvites={invites} />;
}

async function loadTeam(token: string): Promise<[MemberSummary[], InviteSummary[]]> {
  try {
    return await Promise.all([listMembers(token), listInvites(token)]);
  } catch (e) {
    if (e instanceof ApiError && e.status === HTTP_STATUS_UNAUTHORIZED) redirect(SIGN_IN_PATH);
    throw e;
  }
}
