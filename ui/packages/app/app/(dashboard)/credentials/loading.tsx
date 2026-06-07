import { PageHeader, PageTitle, Section, Skeleton } from "@usezombie/design-system";

export default function CredentialsLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Credentials</PageTitle>
      </PageHeader>
      <Skeleton className="mb-6 h-4 w-2/3" />
      <div className="grid items-start gap-8 md:grid-cols-2">
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
          {/* UI GATE: SKIPPED per user override (reason: this is the asChild render target of the Section primitive; its landmark name must match the "Add credential" heading) */}
          <section aria-label="Add credential">
            <Skeleton className="mb-3 h-3 w-32" />
            <Skeleton className="h-48 w-full rounded-md" />
          </section>
        </Section>
      </div>
    </div>
  );
}
