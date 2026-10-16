import { PageLayout, Skeleton } from "@agentsfleet/design-system";

export default function NewFleetLoading() {
  return <PageLayout fullHeight>
    <output aria-label="Loading conversation" className="mx-auto flex w-full max-w-measure flex-1 flex-col items-center justify-center gap-xl">
      <span className="sr-only">Loading conversation</span>
      <Skeleton aria-hidden="true" className="size-10 rounded-full" />
      <Skeleton aria-hidden="true" className="h-8 w-72 max-w-full" />
      <Skeleton aria-hidden="true" className="h-40 w-full rounded-xl" />
      <div aria-hidden="true" className="grid w-full grid-cols-3 gap-md">
        <Skeleton className="h-28 rounded-lg" /><Skeleton className="h-28 rounded-lg" /><Skeleton className="h-28 rounded-lg" />
      </div>
    </output>
  </PageLayout>;
}
