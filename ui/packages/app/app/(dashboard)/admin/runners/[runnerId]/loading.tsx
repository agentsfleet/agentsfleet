import { Skeleton } from "@agentsfleet/design-system";

// The shell's silhouette while the runner and its landing view load: header
// row, identity line, section strip over the table, status line at the foot.
export default function RunnerDetailLoading() {
  return (
    <div className="flex min-h-full flex-1 flex-col">
      <div className="min-w-0">
        <div className="mb-md flex items-center justify-between">
          <Skeleton className="h-5 w-64" />
          <Skeleton className="h-8 w-56" />
        </div>
        <Skeleton className="mb-2xl h-5 w-80" />
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-3xl">
        <div className="flex gap-xs border-b border-border pb-md">
          {Array.from({ length: 2 }, (_, index) => (
            <Skeleton key={index} className="h-9 w-24" />
          ))}
        </div>
        <Skeleton className="h-72 w-full" />
      </div>
      <Skeleton className="mt-auto h-5 w-96" />
    </div>
  );
}
