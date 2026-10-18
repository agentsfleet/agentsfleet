"use client";

import type { Review } from "@/lib/api/draft-types";
import { MetaGrid, Section } from "@agentsfleet/design-system";
import { FleetMarkdownDynamic } from "../FleetMarkdownDynamic";
import { SetupChecks } from "./SetupChecks";
import { SetupSummary } from "./SetupSummary";

export function FleetSetupDetails({review}: {review: Review}) {
  const {effective} = review;
  return <div className="space-y-lg">
    <SetupSummary definition={effective.definition} requirements={review.requirements} />
    <MetaGrid columns={2} items={[
      {label: "Fleet name", value: effective.instance_name},
      {label: "Runtime model", value: effective.runtime_model_ref ?? "Account default is not resolved"},
      ...(effective.source_revision ? [{label: "Source revision", value: effective.source_revision}] : []),
      ...(effective.target_setup_revision ? [{label: "Fleet setup revision", value: effective.target_setup_revision}] : []),
    ]} />
    <Section className="gap-sm" aria-label="Setup instructions">
      <h3 className="font-medium">Instructions</h3>
      {effective.definition.instructions_md ? <FleetMarkdownDynamic>{effective.definition.instructions_md}</FleetMarkdownDynamic>
        : <p className="text-sm text-muted-foreground">Add instructions before saving or publishing.</p>}
    </Section>
    <SetupChecks checks={review.readiness} label="Current readiness" />
  </div>;
}

