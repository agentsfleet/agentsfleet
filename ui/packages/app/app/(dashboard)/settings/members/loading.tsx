import RouteLoading from "@/components/layout/RouteLoading";
import { MEMBERS_DESCRIPTION, MEMBERS_TITLE } from "./copy";

// Paints the real header so the title does not wobble while both lists load.
export default function MembersLoading() {
  return <RouteLoading title={MEMBERS_TITLE} description={MEMBERS_DESCRIPTION} />;
}
