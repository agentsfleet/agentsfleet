import { PageHeader, PageLayout, PageTitle, Skeleton } from "@agentsfleet/design-system";
export default function RouteLoading({
  title,
  description,
  className,
}: {
  title?: string;
  description?: string;
  className?: string;
}) {
  return (
    <PageLayout className={className}>
      {title ? <PageHeader className="flex-col sm:flex-col" description={description}>
        <PageTitle>{title}</PageTitle>
      </PageHeader> : <Skeleton className="h-8 w-48" aria-hidden="true" />}
      <output aria-label={title ? `Loading ${title}` : "Loading page"} className="space-y-lg">
        <span className="sr-only">{title ? `Loading ${title}` : "Loading page"}</span>
        <div aria-hidden="true" className="space-y-lg">
          <Skeleton className="h-10 w-full max-w-sm" />
          <Skeleton className="h-16 w-full rounded-lg" />
          <Skeleton className="h-16 w-full rounded-lg" />
          <Skeleton className="h-16 w-full rounded-lg" />
        </div>
      </output>
    </PageLayout>
  );
}
