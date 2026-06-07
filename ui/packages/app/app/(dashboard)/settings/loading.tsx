import { PageHeader, PageTitle, Skeleton } from "@usezombie/design-system";

export default function SettingsLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Settings</PageTitle>
      </PageHeader>
      <div className="mb-6 flex gap-2">
        {Array.from({ length: 2 }, (_, i) => (
          <Skeleton key={i} className="h-9 w-24" />
        ))}
      </div>
      <Skeleton className="h-64 w-full rounded-md" />
    </div>
  );
}
