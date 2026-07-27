import RouteLoading from "@/components/layout/RouteLoading";
import { INSTALL_PAGE_DESCRIPTION, INSTALL_PAGE_TITLE } from "./library-docs";

// Without this, /fleets/new borrowed the parent fleets loader, which painted
// the wrong "Fleets" title while navigating to the install screen. Match the
// real install header exactly while the gallery and vault reads resolve.
//
// Distinct from the in-page Suspense fallback in `page.tsx`: this covers the
// NAVIGATION into the route, that one covers the gallery region once the shell
// is already on screen.
export default function InstallFleetLoading() {
  return (
    <RouteLoading title={INSTALL_PAGE_TITLE} description={INSTALL_PAGE_DESCRIPTION} />
  );
}
