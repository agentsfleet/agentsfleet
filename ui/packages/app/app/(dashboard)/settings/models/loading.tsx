import { PageHeader, PageTitle, Skeleton } from "@agentsfleet/design-system";

// Without this, /settings/models borrowed the parent settings loader, which
// painted the wrong "Workspace" title on navigation. Show the real Models title
// instantly while the provider + catalogue reads resolve.
export default function ModelsLoading() {
  return (
    <div className="space-y-8">
      <PageHeader description="Choose platform defaults or your key.">
        <PageTitle>Models</PageTitle>
      </PageHeader>
      <div className="grid gap-lg lg:grid-cols-2">
        <Skeleton className="h-56 w-full rounded-lg" />
        <Skeleton className="h-56 w-full rounded-lg" />
      </div>
    </div>
  );
}
