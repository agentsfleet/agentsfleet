import { Spinner } from "@agentsfleet/design-system";

// Dashboard-wide fallback (Next.js loading.tsx): the home route plus any child
// route without its own loader (admin, settings/defaults, settings/security).
// Title-less on purpose — it stands in for many routes, so it shows a neutral
// spinner rather than risk a wrong title. Named routes (fleets, events,
// billing, models, …) have their own titled RouteLoading.
export default function DashboardLoading() {
  return (
    <div aria-busy="true" aria-live="polite">
      <Spinner size="lg" label="Loading…" className="py-16 text-sm" />
    </div>
  );
}
