import { InvitesView } from "../components/InvitesView";
import { loadWaitingInvites } from "../load";

export const dynamic = "force-dynamic";

// Where an invite link lands. The invite may not be among the ones waiting for
// this person's address — it was sent to another one, or it expired — and only
// the accept call can say which, so the page offers it either way.
export default async function InvitePage({ params }: { params: Promise<{ inviteId: string }> }) {
  const { inviteId } = await params;
  return <InvitesView waiting={await loadWaitingInvites()} linkedId={inviteId} />;
}
