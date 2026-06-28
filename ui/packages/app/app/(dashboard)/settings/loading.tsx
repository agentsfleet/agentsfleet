import { PageHeader, PageTitle, Skeleton } from "@agentsfleet/design-system";

// Bespoke skeleton on purpose — NOT the shared RouteLoading (title + spinner).
// The tab strip + content block preview this route's real layout, which a plain
// spinner can't telegraph. RouteLoading is for routes whose loading shape is
// just "title + spinner"; layout-shaped loaders (this, api-keys, fleets/[id],
// approvals/[gateId]) keep their skeleton chrome. The title here is already
// correct ("Workspace"), so there is no wrong-title flash to fix.
export default function SettingsLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Workspace</PageTitle>
      </PageHeader>
      <div className="mb-6 flex gap-2">
        {Array.from({ length: 2 }, (_, i) => (
          <Skeleton key={i} className="h-9 w-24" />
        ))}
      </div>
      <Skeleton className="h-64 w-full rounded-md" />
    </div>
  );
}
