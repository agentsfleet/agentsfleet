import { Fragment } from "react";
import { PageHeader, PageTitle, Section, SectionLabel, Skeleton } from "@agentsfleet/design-system";
import {
  COLUMN_DOES_LABEL,
  COLUMN_IS_LABEL,
  COLUMN_KNOWS_LABEL,
} from "./components/console-copy";

// Mirrors the three-column console (page.tsx) so the skeleton lands in the same
// grid the loaded page fills — no layout shift on hydrate.
const CONSOLE_GRID = "grid grid-cols-1 gap-xl lg:grid-cols-[minmax(0,1fr)_minmax(0,1.5fr)_minmax(0,1fr)]";

export default function FleetDetailLoading() {
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

      <div className={CONSOLE_GRID}>
        {[COLUMN_IS_LABEL, COLUMN_DOES_LABEL, COLUMN_KNOWS_LABEL].map((label) => (
          <Fragment key={label}>
          <Section asChild>
          <section aria-label={label} className="min-w-0">
            <SectionLabel>{label}</SectionLabel>
            <Skeleton className="h-64 w-full rounded-md" />
            <Skeleton className="h-24 w-full rounded-md" />
          </section>
          </Section>
          </Fragment>
        ))}
      </div>
    </div>
  );
}
