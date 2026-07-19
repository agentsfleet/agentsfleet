import { Spinner } from "@agentsfleet/design-system";

import { LoadingVerbLabel } from "@/components/layout/LoadingVerbLabel";
import { loadingAccessibleName } from "@/components/layout/loading-verbs";

// Dashboard-wide fallback (Next.js loading.tsx): the home route plus any child
// route without its own loader (admin, settings/defaults, settings/security).
// Title-less on purpose — it stands in for many routes, so it shows a neutral
// spinner rather than risk a wrong title. Named routes (fleets, events,
// billing, models, …) have their own titled RouteLoading.
export default function DashboardLoading() {
  // Spinner is itself role=status (a polite live region), so the wrapper carries
  // no aria-live/aria-busy — nesting two live regions double-announces.
  return (
    <div>
      <Spinner
        size="lg"
        label={<LoadingVerbLabel />}
        aria-label={loadingAccessibleName()}
        className="py-16 text-sm"
      />
    </div>
  );
}
