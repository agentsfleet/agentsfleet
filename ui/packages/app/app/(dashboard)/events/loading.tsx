import { PageHeader, PageTitle, Skeleton } from "@agentsfleet/design-system";

export default function EventsLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Events</PageTitle>
      </PageHeader>
      <div className="space-y-2">
        {Array.from({ length: 8 }, (_, i) => (
          <Skeleton key={i} className="h-12 w-full rounded-md" />
        ))}
      </div>
    </div>
  );
}
