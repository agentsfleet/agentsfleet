"use client";

import type { Review } from "@/lib/api/draft-types";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, MetaGrid, Section, Terminal } from "@agentsfleet/design-system";
import { FleetMarkdown } from "../FleetMarkdown";
import { SetupChecks } from "./SetupChecks";

const DETAIL = {requirements: "requirements", definition: "definition"} as const;
const REQUIREMENTS_LABEL = "Required access and budget";

export function SetupReviewDetails({review}: {review: Review}) {
  const {effective} = review;
  return <div className="space-y-lg">
    <MetaGrid columns={2} items={[
      {label: "Instance name", value: effective.instance_name},
      {label: "Runtime model", value: effective.runtime_model_ref ?? "Account default is not resolved"},
      ...(effective.source_revision ? [{label: "Source revision", value: effective.source_revision}] : []),
      ...(effective.target_setup_revision ? [{label: "Fleet setup revision", value: effective.target_setup_revision}] : []),
    ]} />
    <Section className="gap-sm" aria-label="Reviewed instructions">
      <h3 className="font-medium">Instructions</h3>
      {effective.definition.instructions_md ? <FleetMarkdown>{effective.definition.instructions_md}</FleetMarkdown>
        : <p className="text-sm text-muted-foreground">Add instructions before saving or publishing.</p>}
    </Section>
    <Accordion type="multiple" defaultValue={[DETAIL.requirements]}>
      <AccordionItem value={DETAIL.requirements}>
        <AccordionTrigger>{REQUIREMENTS_LABEL}</AccordionTrigger>
        <AccordionContent><Terminal label={REQUIREMENTS_LABEL} className="max-h-80 overflow-auto">{JSON.stringify(review.requirements, null, 2)}</Terminal></AccordionContent>
      </AccordionItem>
      <AccordionItem value={DETAIL.definition}>
        <AccordionTrigger>Full setup details</AccordionTrigger>
        <AccordionContent><Terminal label="Native setup" className="max-h-80 overflow-auto">{JSON.stringify(effective.definition, null, 2)}</Terminal></AccordionContent>
      </AccordionItem>
    </Accordion>
    <SetupChecks checks={review.readiness} label="Current readiness" />
  </div>;
}

