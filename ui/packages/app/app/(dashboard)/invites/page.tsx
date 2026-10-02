import { InvitesView } from "./components/InvitesView";
import { loadWaitingInvites } from "./load";

export const dynamic = "force-dynamic";

export default async function InvitesPage() {
  return <InvitesView waiting={await loadWaitingInvites()} linkedId={null} />;
}
