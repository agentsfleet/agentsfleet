import { DashboardPanel, Section, Skeleton } from "@agentsfleet/design-system";

const LOADING_ROWS = 5;

export default function Loading() {
  return (
    <Section>
      <output className="sr-only" aria-label="Loading connected apps" aria-busy="true">Loading connected apps</output>
      <DashboardPanel padding="none" className="divide-y divide-border" aria-hidden="true">
        {Array.from({ length: LOADING_ROWS }, (_, row) => (
          <div key={row} className="p-xl sm:p-5">
            <div className="flex min-w-0 flex-wrap items-center gap-xl">
              <Skeleton className="hidden size-10 shrink-0 animate-none rounded-control border border-border sm:block" />
              <div className="min-w-0 basis-full sm:flex-1 sm:basis-auto">
                <Skeleton className="h-4 w-24 animate-none rounded-control" />
                <Skeleton className="mt-sm h-4 w-32 animate-none rounded-control" />
              </div>
              <Skeleton className="h-[var(--control-height)] w-24 animate-none rounded-control border border-border" />
            </div>
          </div>
        ))}
      </DashboardPanel>
    </Section>
  );
}
