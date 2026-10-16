"use client";

import { useRouter } from "next/navigation";
import { PageHeader, PageLayout, PageTitle, Section } from "@agentsfleet/design-system";
import type { RunnerListResponse } from "@/lib/api/runners";
import RunnerWall from "./RunnerWall";
import AddRunnerDialogDynamic from "@/components/domain/island-dynamic/AddRunnerDialogDynamic";

// Brief "what a runner is" — the install-token minting is explained in the
// Create-runner dialog (as a shown-once alert), not repeated on the page.
const RUNNERS_DESCRIPTION = "Hosts you enroll to run fleets.";
const RUNNERS_SECTION_LABEL = "Runners";

// The wall renders the server-fetched page; a newly-enrolled runner appears by
// refreshing the route (the wall is newest-first, so it lands at the top).
export default function RunnersView({ initial, canEnroll }: { initial: RunnerListResponse; canEnroll: boolean }) {
  const router = useRouter();
  return (
    <PageLayout>
      <PageHeader description={RUNNERS_DESCRIPTION} actions={canEnroll ? <AddRunnerDialogDynamic onCreated={() => router.refresh()} /> : undefined}>
        <PageTitle>{RUNNERS_SECTION_LABEL}</PageTitle>
      </PageHeader>

      <Section asChild>
        {/* UI GATE: SKIPPED per user override (reason: sanctioned <Section asChild> wrap opens on the unchanged preceding line; only the aria-label value changed) */}
        <section aria-label={RUNNERS_SECTION_LABEL}>
          <RunnerWall initialRunners={initial.items} initialCursor={initial.next_cursor} />
        </section>
      </Section>
    </PageLayout>
  );
}
