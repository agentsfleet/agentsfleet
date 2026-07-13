import { redirect } from "next/navigation";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { NOT_PLATFORM_ADMIN } from "./library-copy";
import FleetLibrariesView from "./components/FleetLibrariesView";

export const dynamic = "force-dynamic";

export default async function AdminFleetLibrariesPage() {
  // Platform library operators only. The backend independently 403s a token
  // missing the scope (UZ-AUTH-022); this is the UI guard, so a non-operator
  // never sees an action they cannot take.
  //
  // Unlike the models surface there is no read here: the platform catalog has
  // no list route, so the page renders the onboard affordance and nothing else
  // until the operator acts. Verification is the workspace gallery.
  if (!(await hasScope(SCOPE.PLATFORM_LIBRARY_WRITE))) redirect(NOT_PLATFORM_ADMIN);

  return <FleetLibrariesView />;
}
