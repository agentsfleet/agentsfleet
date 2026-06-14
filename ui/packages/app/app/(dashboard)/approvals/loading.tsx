import { PageHeader, PageTitle, Skeleton } from "@agentsfleet/design-system";

export default function ApprovalsLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Approvals</PageTitle>
      </PageHeader>
      <div className="space-y-2">
        {Array.from({ length: 5 }, (_, i) => (
          <Skeleton key={i} className="h-16 w-full rounded-md" />
        ))}
      </div>
    </div>
  );
}
