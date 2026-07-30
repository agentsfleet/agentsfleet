import { Card, Skeleton } from "@agentsfleet/design-system";

// The shell's silhouette while the runner and its landing view load: header
// row, identity line, rail beside the strip-over-table pane.
export default function RunnerDetailLoading() {
  return (
    <div className="flex min-h-full flex-1 flex-col">
      <div className="flex min-w-0 flex-col gap-3xl lg:flex-row">
        <div aria-hidden="true" className="hidden lg:block lg:w-56 lg:shrink-0" />
        <div className="min-w-0 flex-1">
          <div className="mb-md flex items-center justify-between">
            <Skeleton className="h-5 w-64" />
            <Skeleton className="h-8 w-56" />
          </div>
          <Skeleton className="mb-2xl h-5 w-80" />
        </div>
      </div>
      <div className="flex min-w-0 flex-1 flex-col gap-3xl lg:flex-row">
        <div className="hidden lg:block lg:w-56 lg:shrink-0">
          <Skeleton className="h-24 w-full" />
        </div>
        <div className="flex min-w-0 flex-1 flex-col gap-3xl">
          <Card className="p-lg">
            <Skeleton className="h-16 w-full" />
          </Card>
          <Skeleton className="h-72 w-full" />
        </div>
      </div>
    </div>
  );
}
