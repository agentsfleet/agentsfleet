import { PageHeader, PageTitle, Skeleton } from "@agentsfleet/design-system";

// Paints the correct Credentials title instantly on navigation while the
// provider + stored-secret reads resolve.
export default function CredentialsLoading() {
  return (
    <div className="space-y-8">
      <PageHeader description="Write-only keys for models, tools, and secrets.">
        <PageTitle>Credentials</PageTitle>
      </PageHeader>
      <Skeleton className="h-24 w-full rounded-lg" />
      <Skeleton className="h-48 w-full rounded-lg" />
    </div>
  );
}
