import { PageHeader, PageTitle, Skeleton } from "@agentsfleet/design-system";

// Route-level fallback: paints the correct title instantly on navigation while
// the connector status + stored-secret reads resolve, so the header never
// flashes a wrong/empty title.
export default function IntegrationsLoading() {
  return (
    <div className="space-y-8">
      <PageHeader description="Connect the tools your fleets act through.">
        <PageTitle>Integrations</PageTitle>
      </PageHeader>
      <div className="space-y-3">
        {Array.from({ length: 3 }, (_, i) => (
          <Skeleton key={i} className="h-16 w-full rounded-lg" />
        ))}
      </div>
    </div>
  );
}
