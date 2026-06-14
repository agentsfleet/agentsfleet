"use client";

import { useRef } from "react";
import { PageHeader, PageTitle, Section } from "@agentsfleet/design-system";
import type { RunnerListResponse } from "@/lib/api/runners";
import RunnerList, { type RunnerListHandle } from "./RunnerList";
import AddRunnerDialog from "./AddRunnerDialog";

// Client wrapper so the header "Add runner" action and the list can share a
// refresh without a full-route reload: the dialog calls the list's ref on
// create, which re-fetches just the list (page 1) via its Server Action.
export default function RunnersView({ initial }: { initial: RunnerListResponse }) {
  const listRef = useRef<RunnerListHandle>(null);
  return (
    <div>
      <PageHeader>
        <PageTitle>Runners</PageTitle>
        <AddRunnerDialog onCreated={() => listRef.current?.refresh()} />
      </PageHeader>
      <p className="mb-6 max-w-2xl text-sm text-muted-foreground">
        Runners are hosts you enroll to execute agent work. Adding one mints an install token
        that&apos;s shown only once.
      </p>
      <Section asChild>
        <section aria-label="Runners">
          <RunnerList ref={listRef} initial={initial} />
        </section>
      </Section>
    </div>
  );
}
