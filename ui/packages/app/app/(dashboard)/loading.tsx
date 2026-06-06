import { Skeleton } from "@usezombie/design-system";

// Dashboard-wide fallback skeleton. Routes without their own loading.tsx
// (Dashboard, Runners) get instant feedback on navigation instead of hanging
// on the previous page while the server renders. Closer loading.tsx boundaries
// (settings, zombies, credentials, events, approvals, api-keys) still win.
export default function DashboardLoading() {
  return (
    <div className="space-y-6" aria-busy="true" aria-live="polite">
      <Skeleton className="h-8 w-48" />
      <Skeleton className="h-64 w-full rounded-md" />
    </div>
  );
}
