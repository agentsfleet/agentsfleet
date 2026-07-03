import { PageHeader, PageTitle, Section, Skeleton } from "@agentsfleet/design-system";

// Bespoke skeleton on purpose — NOT the shared RouteLoading (title + spinner).
// The header + key-list rows preview the real table layout, which a plain
// spinner can't. Mirrors the loaded view: the "Workspace" title + tab strip
// paint first (same as settings/loading), so there is no wrong-title flash.
export default function ApiKeysLoading() {
  return (
    <div>
      <PageHeader>
        <PageTitle>Workspace</PageTitle>
      </PageHeader>
      <div className="mb-6 flex gap-2">
        {Array.from({ length: 2 }, (_, i) => (
          <Skeleton key={i} className="h-9 w-24" />
        ))}
      </div>
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
