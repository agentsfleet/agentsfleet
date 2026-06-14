import { PageHeader, PageTitle, Section, SectionLabel, Skeleton } from "@agentsfleet/design-system";

export default function ZombieDetailLoading() {
  return (
    <div>
      <PageHeader>
        <div className="flex items-center gap-3">
          <PageTitle>
            <Skeleton className="h-6 w-48" />
          </PageTitle>
          <Skeleton className="h-4 w-16" />
        </div>
        <Skeleton className="h-9 w-20" />
      </PageHeader>

      {["Trigger", "Configuration", "Pending approvals", "Live activity", "Recent Activity"].map((label) => (
        <Section asChild key={label}>
          <section aria-label={label}>
            <SectionLabel>{label}</SectionLabel>
            <Skeleton className="h-24 w-full rounded-md" />
          </section>
        </Section>
      ))}
    </div>
  );
}
