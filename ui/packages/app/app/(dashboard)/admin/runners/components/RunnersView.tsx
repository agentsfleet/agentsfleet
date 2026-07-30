"use client";

import { useRouter } from "next/navigation";
import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import type { RunnerListResponse } from "@/lib/api/runners";
import RunnerWall from "./RunnerWall";
import AddRunnerDialogDynamic from "@/components/domain/island-dynamic/AddRunnerDialogDynamic";

// Brief "what a runner is" — the install-token minting is explained in the
// Create-runner dialog (as a shown-once alert), not repeated on the page.
const RUNNERS_DESCRIPTION = "Hosts you enroll to run fleets.";
const RUNNERS_SECTION_LABEL = "Runners";
const RUNNERS_SECTION_HEADER = "Manage runners";

// The wall renders the server-fetched page; a newly-enrolled runner appears by
// refreshing the route (the wall is newest-first, so it lands at the top).
export default function RunnersView({ initial }: { initial: RunnerListResponse }) {
  const router = useRouter();
  return (
    <PageLayout>
      <PageHeader description={RUNNERS_DESCRIPTION}>
        <PageTitle>{RUNNERS_SECTION_LABEL}</PageTitle>
      </PageHeader>

      <Section asChild>
        {/* UI GATE: SKIPPED per user override (reason: sanctioned <Section asChild> wrap opens on the unchanged preceding line; only the aria-label value changed) */}
        <section aria-label={RUNNERS_SECTION_LABEL}>
          <SectionHeader actions={<AddRunnerDialogDynamic onCreated={() => router.refresh()} />}>
            {RUNNERS_SECTION_HEADER}
          </SectionHeader>
          <RunnerWall initialRunners={initial.items} initialCursor={initial.next_cursor} />
        </section>
      </Section>
    </PageLayout>
  );
}
