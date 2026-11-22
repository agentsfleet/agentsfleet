"use client";

import type { Definition, Review } from "@/lib/api/draft-types";
import { DashboardPanel, MetaGrid, Section } from "@agentsfleet/design-system";
import { FleetMarkdownDynamic } from "../../shared/chat/FleetMarkdownDynamic";
import { SetupChecks } from "./SetupChecks";
import { SetupSummary } from "./SetupSummary";
import { savedSetupRequirements } from "@/lib/setup-requirements";
import { SetupQuestions } from "./SetupQuestions";

export function FleetSetupDetails({review, definition: savedDefinition, name, revision, sourceRevision}: {review?: Review; definition?: Definition; name?: string; revision?: string; sourceRevision?: string}) {
  const effective = review?.effective;
  const definition = effective?.definition ?? savedDefinition;
  const config = savedSetupRequirements(definition);
  const requirements = review?.requirements ?? config ?? undefined;
  const savedRevision = revision ?? effective?.target_setup_revision;
  const runtimeModel = effective ? effective.runtime_model_ref ?? "Account default is not resolved"
    : config?.model ?? "Runtime model unavailable in this view";
  return <div className="min-w-0 space-y-2xl text-label leading-label [overflow-wrap:anywhere]">
    <Section className="gap-lg" aria-label="Fleet identity">
      <h3 className="font-semibold">Fleet</h3>
      <DashboardPanel padding="none"><MetaGrid className="block divide-y divide-border [&>div]:p-lg [&_dt]:text-label [&_dt]:normal-case [&_dt]:tracking-normal [&_dd]:text-label" items={[
        {label: "Name", value: effective?.instance_name ?? name ?? "New fleet"},
        {label: "Runtime model", value: runtimeModel},
        {label: "Source revision", value: effective?.source_revision ?? sourceRevision ?? "Source revision unavailable"},
        ...(savedRevision ? [{label: "Current saved setup", value: `Revision ${savedRevision}`}] : []),
      ]} /></DashboardPanel>
      {revision && effective?.target_setup_revision && revision !== effective.target_setup_revision ? <p className="text-warning">This proposal was reviewed against revision {effective.target_setup_revision}. The saved fleet is now on revision {revision}; refresh the setup before saving.</p> : null}
    </Section>
    <SetupSummary definition={definition} requirements={requirements} modelLabel={runtimeModel} />
    {definition ? <SetupQuestions definition={definition} /> : null}
    <details className="rounded-lg border border-border">
      <summary className="min-h-11 cursor-pointer rounded-control p-md font-medium focus-visible:outline-2 focus-visible:outline-ring sm:min-h-9 pointer-coarse:min-h-11">Instructions</summary>
      <div className="px-lg pb-lg">
      {definition?.instructions_md ? <FleetMarkdownDynamic>{definition.instructions_md}</FleetMarkdownDynamic>
        : <p className="text-sm text-muted-foreground">Add instructions before saving.</p>}
      </div>
    </details>
    {review ? <SetupChecks checks={review.readiness} label="Current readiness" /> : null}
  </div>;
}

