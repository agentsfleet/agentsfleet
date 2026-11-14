"use client";

import type { Definition, Review } from "@/lib/api/draft-types";
import { z } from "zod";
import { DashboardPanel, MetaGrid, Section } from "@agentsfleet/design-system";
import { FleetMarkdownDynamic } from "../FleetMarkdownDynamic";
import { SetupChecks } from "./SetupChecks";
import { SetupSummary } from "./SetupSummary";
import { SetupRequirementsSchema } from "./SetupAccessSummary";

// Normalized saved definitions omit empty credential/repository lists.
const SavedConfigSchema = SetupRequirementsSchema.extend({
  credentials: z.array(z.string()).default([]), repositories: z.array(z.string()).default([]), model: z.string().optional(),
});

export function FleetSetupDetails({review, definition: savedDefinition, name, revision, sourceRevision}: {review?: Review; definition?: Definition; name?: string; revision?: string; sourceRevision?: string}) {
  const effective = review?.effective;
  const definition = effective?.definition ?? savedDefinition;
  const config = SavedConfigSchema.safeParse(definition?.config["x-agentsfleet"]);
  const requirements = review?.requirements ?? (config.success ? config.data : undefined);
  const savedRevision = revision ?? effective?.target_setup_revision;
  const runtimeModel = effective ? effective.runtime_model_ref ?? "Account default is not resolved"
    : (config.success ? config.data.model : undefined) ?? "Runtime model unavailable in this view";
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
    <SetupSummary definition={definition} requirements={requirements} />
    <details className="rounded-lg border border-border">
      <summary className="min-h-11 cursor-pointer rounded-control p-md font-medium focus-visible:outline-2 focus-visible:outline-ring sm:min-h-9 pointer-coarse:min-h-11">Instructions</summary>
      <div className="px-lg pb-lg">
      {definition?.instructions_md ? <FleetMarkdownDynamic>{definition.instructions_md}</FleetMarkdownDynamic>
        : <p className="text-sm text-muted-foreground">Add instructions before saving or publishing.</p>}
      </div>
    </details>
    {review ? <SetupChecks checks={review.readiness} label="Current readiness" /> : null}
  </div>;
}

