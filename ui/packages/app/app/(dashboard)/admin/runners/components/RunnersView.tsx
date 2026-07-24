"use client";

import { useRef } from "react";
import { PageHeader, PageLayout, PageTitle, Section, SectionHeader } from "@agentsfleet/design-system";
import type { RunnerListResponse } from "@/lib/api/runners";
import RunnerList, { type RunnerListHandle } from "./RunnerList";
import AddRunnerDialogDynamic from "@/components/domain/island-dynamic/AddRunnerDialogDynamic";

// Brief "what a runner is" — the install-token minting is explained in the
// Create-runner dialog (as a shown-once alert), not repeated on the page.
const RUNNERS_DESCRIPTION = "Hosts you enroll to run fleets.";

// Client wrapper so the "Create runner" action and the list share a refresh
// without a full-route reload: the dialog calls the list's ref on create, which
// re-fetches just the list (page 1) via its Server Action.
export default function RunnersView({ initial }: { initial: RunnerListResponse }) {
  const listRef = useRef<RunnerListHandle>(null);
  return (
    <PageLayout>
      <PageHeader description={RUNNERS_DESCRIPTION}>
        <PageTitle>Runners</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Runners">
          <SectionHeader actions={<AddRunnerDialogDynamic onCreated={() => listRef.current?.refresh()} />}>
            Manage runners
          </SectionHeader>
          <RunnerList ref={listRef} initial={initial} />
        </section>
      </Section>
    </PageLayout>
  );
}
