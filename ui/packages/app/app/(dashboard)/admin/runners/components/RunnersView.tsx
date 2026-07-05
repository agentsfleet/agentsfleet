"use client";

import { useRef } from "react";
import { PageHeader, PageTitle, Section, SectionLabel } from "@agentsfleet/design-system";
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
    <div className="space-y-8">
      <PageHeader description={RUNNERS_DESCRIPTION}>
        <PageTitle>Runners</PageTitle>
      </PageHeader>

      <Section asChild>
        <section aria-label="Runners">
          <div className="flex flex-wrap items-baseline justify-between gap-md">
            <SectionLabel>Manage runners</SectionLabel>
            <AddRunnerDialogDynamic onCreated={() => listRef.current?.refresh()} />
          </div>
          <RunnerList ref={listRef} initial={initial} />
        </section>
      </Section>
    </div>
  );
}
