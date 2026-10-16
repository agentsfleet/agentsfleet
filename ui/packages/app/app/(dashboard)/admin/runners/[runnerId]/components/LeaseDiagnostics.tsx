import type { ReactNode } from "react";
import type { RunnerLease } from "@/lib/api/runners";
import { LEASE_KIND } from "@/lib/api/runners";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger,
  DescriptionDetails, DescriptionList, DescriptionTerm } from "@agentsfleet/design-system";
import { REVIEW_EVENT_LABEL, REVIEW_FENCING_LABEL, REVIEW_KIND_LABEL,
  REVIEW_LEASE_ID_LABEL, REVIEW_POSTURE_LABEL, REVIEW_TOKENS_LABEL } from "./runner-copy";

const COUNT = new Intl.NumberFormat("en-US");
const KIND_LABELS = { [LEASE_KIND.fresh]: "New work", [LEASE_KIND.reclaim]: "Resumed work" } as const;
const POSTURE_LABELS: Record<string, string> = { platform: "Platform managed", self_managed: "Workspace managed", metered: "Metered" };

export function LeaseFact({ label, children, mono = false }: { label: string; children: ReactNode; mono?: boolean }) {
  return <div className="min-w-0">
    <DescriptionTerm>{label}</DescriptionTerm>
    <DescriptionDetails mono={mono} className="min-w-0 break-all">{children}</DescriptionDetails>
  </div>;
}

export function LeaseDiagnostics({ lease }: { lease: RunnerLease }) {
  return <Accordion type="single" collapsible>
    <AccordionItem value="diagnostics">
      <AccordionTrigger>Technical details</AccordionTrigger>
      <AccordionContent>
        <DescriptionList className="font-sans text-body-sm tabular-nums">
          <LeaseFact label={REVIEW_LEASE_ID_LABEL} mono>{lease.id}</LeaseFact>
          <LeaseFact label={REVIEW_KIND_LABEL}>{KIND_LABELS[lease.kind]}</LeaseFact>
          <LeaseFact label={REVIEW_FENCING_LABEL} mono>{COUNT.format(lease.fencing_token)}</LeaseFact>
          <LeaseFact label={REVIEW_POSTURE_LABEL}>{POSTURE_LABELS[lease.posture] ?? lease.posture}</LeaseFact>
          <LeaseFact label={REVIEW_TOKENS_LABEL}>{COUNT.format(lease.metered_input_tokens)} in · {COUNT.format(lease.metered_cached_tokens)} cached · {COUNT.format(lease.metered_output_tokens)} out</LeaseFact>
          <LeaseFact label={REVIEW_EVENT_LABEL} mono>{lease.event_id}</LeaseFact>
        </DescriptionList>
      </AccordionContent>
    </AccordionItem>
  </Accordion>;
}
