import { PageLayout, Skeleton } from "@agentsfleet/design-system";

export default function FleetsLoading() {
  return <PageLayout>
    <output aria-label="Loading Fleets" className="space-y-2xl">
      <span className="sr-only">Loading Fleets</span>
      <div aria-hidden="true" className="flex flex-col items-center gap-lg py-2xl">
        <Skeleton className="size-10 rounded-full" /><Skeleton className="h-8 w-72 max-w-full" /><Skeleton className="h-10 w-32 rounded-lg" />
      </div>
      <div aria-hidden="true" className="grid grid-cols-1 gap-lg sm:grid-cols-2 xl:grid-cols-3">
        {Array.from({ length: 6 }, (_, index) => <Skeleton key={index} className="h-48 rounded-xl" />)}
      </div>
    </output>
  </PageLayout>;
}
