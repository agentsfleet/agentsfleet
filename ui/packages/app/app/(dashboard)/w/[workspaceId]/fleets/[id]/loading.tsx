import { PageHeader, PageLayout, PageTitle, Skeleton } from "@agentsfleet/design-system";

export default function FleetDetailLoading() {
  return (
    <PageLayout>
      <div className="space-y-sm">
        <Skeleton className="h-4 w-56" />
        <PageHeader>
          <div className="flex items-center gap-md">
            <PageTitle>
              <Skeleton className="h-6 w-48" />
            </PageTitle>
            <Skeleton className="h-4 w-16" />
          </div>
        </PageHeader>
      </div>
      <div className="flex flex-col gap-xl lg:flex-row">
        <div className="flex gap-xs border-b border-border pb-md lg:w-48 lg:flex-col lg:border-b-0 lg:border-r lg:pr-lg">
          {Array.from({ length: 6 }, (_, index) => (
            <Skeleton key={index} className="h-9 w-24 lg:w-40" />
          ))}
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-lg">
          <Skeleton className="h-28 w-full rounded-lg" />
          <Skeleton className="h-96 w-full rounded-lg" />
        </div>
      </div>
    </PageLayout>
  );
}
