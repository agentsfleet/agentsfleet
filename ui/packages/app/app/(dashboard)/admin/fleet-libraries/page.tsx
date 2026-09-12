import { redirect } from "next/navigation";
import Link from "next/link";
import { Alert, Button, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { hasScope } from "@/lib/auth/platform";
import { SCOPE } from "@/lib/auth/scopes";
import { withToken } from "@/lib/actions/with-token";
import { listPlatformFleetLibrary } from "@/lib/api/fleet-library";
import { presentErrorString } from "@/lib/errors";
import { ADMIN_FLEET_LIBRARIES_PATH, CATALOG_READ_ACTION, FLEET_LIBRARIES_DESCRIPTION, FLEET_LIBRARY_TITLE, NOT_PLATFORM_ADMIN } from "./library-copy";
import FleetLibrariesView from "./components/FleetLibrariesView";

export const dynamic = "force-dynamic";

// The onboard action on this page waits on the daemon's own GitHub fetch, so it
// outlives an ordinary render. Without this the function takes the platform
// default, which is shorter than the budget the client grants the request
// (`ONBOARD_BUNDLE_TIMEOUT_MS`, 30s) — the operator would get an opaque
// platform 504 instead of the dialog's own message. 60 leaves the client's
// budget room to expire first and stays inside every plan's ceiling.
// A literal, because route segment config must be statically analysable.
export const maxDuration = 60;

export default async function AdminFleetLibrariesPage() {
  // Platform library operators only. The backend independently 403s a token
  // missing the scope (UZ-AUTH-022); this is the UI guard, so a non-operator
  // never sees an action they cannot take.
  if (!(await hasScope(SCOPE.PLATFORM_LIBRARY_WRITE))) redirect(NOT_PLATFORM_ADMIN);

  const result = await withToken((t) => listPlatformFleetLibrary(t));

  // A failed read renders the failure. It must NEVER fall through to an empty
  // table: "the catalog is empty" and "we could not reach the catalog" are
  // different facts, and an operator acts differently on each.
  if (!result.ok) {
    return (
      <PageLayout>
        <PageHeader description={FLEET_LIBRARIES_DESCRIPTION}><PageTitle>{FLEET_LIBRARY_TITLE}</PageTitle></PageHeader>
        <Alert variant="destructive">
        {presentErrorString({
          errorCode: result.errorCode,
          message: result.error,
          action: CATALOG_READ_ACTION,
        })}
        </Alert>
        <Button asChild className="justify-self-start"><Link href={ADMIN_FLEET_LIBRARIES_PATH}>Retry fleet library</Link></Button>
      </PageLayout>
    );
  }

  return <FleetLibrariesView entries={result.data.entries} />;
}
