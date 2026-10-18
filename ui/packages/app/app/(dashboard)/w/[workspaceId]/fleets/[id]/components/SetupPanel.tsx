import Link from "next/link";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Badge, Button, DashboardPanelTitle, Section, Terminal } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import type { FleetDetail } from "@/lib/types";
import { FleetMarkdownDynamic } from "@/components/domain/FleetMarkdownDynamic";

const SETUP_COPY = {
  region: "Fleet setup",
  title: "What this fleet does",
  description: "These are the saved instructions your fleet follows.",
  edit: "Edit fleet",
  editHint: "Opens a private setup chat. Review changes before applying them.",
  instructions: "Saved instructions",
  advanced: "Advanced settings",
  configuration: "Saved configuration",
  version: "Saved version",
  updateHint: "Updates apply to new work. Work already running keeps its current instructions.",
} as const;

export function SetupPanel({workspaceId, fleet}: {workspaceId: string; fleet: FleetDetail}) {
  const query = new URLSearchParams({fleet: fleet.id, setup_revision: fleet.setup_revision});
  return <Section aria-label={SETUP_COPY.region} className="gap-xl">
    <div className="space-y-md">
      <div className="space-y-sm"><DashboardPanelTitle>{SETUP_COPY.title}</DashboardPanelTitle>
        <p className="text-sm text-muted-foreground">{SETUP_COPY.description}</p></div>
      <div className="flex flex-wrap items-center gap-md">
        <Button asChild size="lg"><Link href={`${workspacePath(workspaceId, "fleets/new")}?${query}`}>{SETUP_COPY.edit}</Link></Button>
        <p className="max-w-form text-sm text-muted-foreground">{SETUP_COPY.editHint}</p>
      </div>
    </div>
    {fleet.needs_re_review ? <Badge variant="amber">Review current access before resuming</Badge> : null}
    <Section aria-label={SETUP_COPY.instructions} className="gap-sm border-l-2 border-primary/30 pl-lg">
      <h3 className="text-label font-semibold text-muted-foreground">{SETUP_COPY.instructions}</h3>
      <FleetMarkdownDynamic>{fleet.definition.instructions_md}</FleetMarkdownDynamic>
    </Section>
    <Accordion type="single" collapsible>
      <AccordionItem value="configuration">
        <AccordionTrigger>{SETUP_COPY.advanced}</AccordionTrigger>
        <AccordionContent><div className="space-y-md">
          <p className="text-sm text-muted-foreground">{SETUP_COPY.version} {fleet.setup_revision}. {SETUP_COPY.updateHint}</p>
          <Terminal label={SETUP_COPY.configuration} className="max-h-96 overflow-auto">{JSON.stringify(fleet.definition.config, null, 2)}</Terminal>
        </div></AccordionContent>
      </AccordionItem>
    </Accordion>
  </Section>;
}
