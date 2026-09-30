import RouteLoading from "@/components/layout/RouteLoading";
import { INVITES_DESCRIPTION, INVITES_TITLE } from "./copy";

// Covers both Invites routes: the linked one nests under this segment.
export default function InvitesLoading() {
  return <RouteLoading title={INVITES_TITLE} description={INVITES_DESCRIPTION} />;
}
