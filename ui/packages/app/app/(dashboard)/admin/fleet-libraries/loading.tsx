import RouteLoading from "@/components/layout/RouteLoading";
import { FLEET_LIBRARIES_TITLE, FLEET_LIBRARIES_DESCRIPTION } from "./library-copy";

// Paint the real header immediately so the title doesn't wobble on navigation,
// matching the other platform routes.
export default function FleetLibrariesLoading() {
  return <RouteLoading title={FLEET_LIBRARIES_TITLE} description={FLEET_LIBRARIES_DESCRIPTION} />;
}
