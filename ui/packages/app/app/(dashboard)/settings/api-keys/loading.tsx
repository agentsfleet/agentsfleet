import { PageHeader, PageTitle, Section, Skeleton } from "@agentsfleet/design-system";

// Bespoke skeleton on purpose — NOT the shared RouteLoading (title + spinner).
// The header + key-list rows preview the real table layout, which a plain
// spinner can't. Same rationale as settings/loading; the "API keys" title is
// already correct, so there is no wrong-title flash to fix.
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
