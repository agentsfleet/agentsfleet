import { DashboardPanel, Grid, PageHeader, PageLayout, PageTitle, Skeleton } from "@agentsfleet/design-system";
import type { ReactNode } from "react";
export default function RouteLoading({
  title,
  description,
  className,
  layout = "table",
}: {
  title?: string;
  description?: ReactNode;
  className?: string;
  layout?: "table" | "cards";
}) {
  return (
    <PageLayout className={className}>
      {title ? <PageHeader className="flex-col sm:flex-col" description={description}>
        <PageTitle>{title}</PageTitle>
      </PageHeader> : <Skeleton className="h-8 w-48" aria-hidden="true" />}
      <output aria-label={title ? `Loading ${title}` : "Loading page"} className="space-y-lg">
        <span className="sr-only">{title ? `Loading ${title}` : "Loading page"}</span>
        <div aria-hidden="true" className="space-y-xl">
          <Skeleton className="h-9 w-full max-w-sm" />
          {layout === "cards" ? <Grid columns="two" className="grid-cols-1 lg:grid-cols-2">
            {Array.from({ length: 4 }, (_, index) => <DashboardPanel key={index} className="space-y-xl p-2xl">
              <div className="flex items-center gap-lg"><Skeleton className="size-10 rounded-lg" /><Skeleton className="h-5 w-32" /></div>
              <Skeleton className="h-16 w-full" />
              <div className="flex justify-end border-t border-border pt-lg"><Skeleton className="h-8 w-24" /></div>
            </DashboardPanel>)}
          </Grid> : <div className="overflow-hidden rounded-lg border border-border">
            <Skeleton className="h-11 w-full rounded-none" />
            <div className="space-y-lg p-lg"><Skeleton className="h-5 w-full" /><Skeleton className="h-5 w-full" /><Skeleton className="h-5 w-full" /></div>
            <Skeleton className="h-12 w-full rounded-none" />
          </div>}
        </div>
      </output>
    </PageLayout>
  );
}
