import { PageHeader, PageTitle, Section, Skeleton } from "@usezombie/design-system";

export default function CredentialsLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Credentials</PageTitle>
      </PageHeader>
      <Skeleton className="mb-6 h-4 w-2/3" />
      <div className="grid gap-8 md:grid-cols-2">
        <Section asChild>
          <section aria-label="Stored credentials">
            <Skeleton className="mb-3 h-3 w-32" />
            <div className="space-y-1">
              {Array.from({ length: 3 }, (_, i) => (
                <Skeleton key={i} className="h-12 w-full rounded-md" />
              ))}
            </div>
          </section>
        </Section>
        <Section asChild>
          <section aria-label="Add a credential">
            <Skeleton className="mb-3 h-3 w-32" />
            <Skeleton className="h-48 w-full rounded-md" />
          </section>
        </Section>
      </div>
    </div>
  );
}
