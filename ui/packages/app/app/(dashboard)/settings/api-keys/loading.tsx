import { PageHeader, PageLayout, PageTitle, Section, Skeleton } from "@agentsfleet/design-system";

// Bespoke skeleton on purpose — NOT the shared RouteLoading (title + spinner).
// The key-list rows preview the real page's layout, which a plain spinner
// can't. Mirrors the loaded view (no tab strip since the Workspace tab
// folded into this page), so there is no wrong-title flash.
export default function ApiKeysLoading() {
  return (
    <PageLayout>
      <PageHeader>
        <PageTitle>API Keys</PageTitle>
      </PageHeader>
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
    </PageLayout>
  );
}
