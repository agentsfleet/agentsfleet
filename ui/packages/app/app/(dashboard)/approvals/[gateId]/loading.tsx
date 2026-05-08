import { PageHeader, PageTitle, Skeleton } from "@usezombie/design-system";

export default function ApprovalDetailLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>
          <Skeleton className="h-6 w-40" />
        </PageTitle>
      </PageHeader>
      <Skeleton className="h-48 w-full rounded-md" />
      <div className="mt-4 flex gap-2">
        <Skeleton className="h-9 w-24" />
        <Skeleton className="h-9 w-24" />
      </div>
    </div>
  );
}
