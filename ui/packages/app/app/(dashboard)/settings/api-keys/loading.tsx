import { PageHeader, PageTitle, Section, Skeleton } from "@usezombie/design-system";

export default function ApiKeysLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>API keys</PageTitle>
      </PageHeader>
      <Skeleton className="mb-6 h-4 w-2/3" />
      <Section asChild>
        <section aria-label="API keys">
          <div className="mb-3 flex items-center justify-between">
            <Skeleton className="h-3 w-32" />
            <Skeleton className="h-8 w-28 rounded-md" />
          </div>
          <div className="space-y-1">
            {Array.from({ length: 3 }, (_, i) => (
              <Skeleton key={i} className="h-14 w-full rounded-md" />
            ))}
          </div>
        </section>
      </Section>
    </div>
  );
}
