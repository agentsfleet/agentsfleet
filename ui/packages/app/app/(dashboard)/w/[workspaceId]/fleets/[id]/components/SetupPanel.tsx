import Link from "next/link";
import { Badge, Button, DashboardPanelTitle, Section } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import type { FleetDetail } from "@/lib/types";
import { FleetMarkdownDynamic } from "@/components/domain/FleetMarkdownDynamic";
import TriggerPanel from "./TriggerPanel";

const SETUP_COPY = {
  region: "Fleet setup",
  title: "What this fleet does",
  edit: "Edit fleet",
  editHint: "Opens a private chat for this saved setup. Save changes there to update new work.",
  instructions: "Saved instructions",
  version: "Saved version",
  updateHint: "Updates apply to new work. Work already running keeps its current instructions.",
} as const;

export function SetupPanel({workspaceId, fleet, lastDeliveryByKey}: {workspaceId: string; fleet: FleetDetail; lastDeliveryByKey?: Record<string, string | null | undefined>}) {
  const query = new URLSearchParams({fleet: fleet.id, setup_revision: fleet.setup_revision});
  return <Section aria-label={SETUP_COPY.region} className="gap-xl">
    <div className="space-y-md">
      <div className="space-y-sm"><DashboardPanelTitle>{SETUP_COPY.title}</DashboardPanelTitle>
        <p className="break-words text-heading font-semibold">{fleet.definition.description}</p>
        <p className="text-sm text-muted-foreground">{SETUP_COPY.version} {fleet.setup_revision}. {SETUP_COPY.updateHint}</p></div>
      <div className="flex flex-wrap items-center gap-md">
        <Button asChild size="lg"><Link href={`${workspacePath(workspaceId, "fleets/new")}?${query}`}>{SETUP_COPY.edit}</Link></Button>
        <p className="max-w-form text-sm text-muted-foreground">{SETUP_COPY.editHint}</p>
      </div>
    </div>
    {fleet.needs_re_review ? <Badge variant="amber">Access needs attention before resuming</Badge> : null}
    <Section aria-label={SETUP_COPY.instructions} className="gap-sm border-l-2 border-primary/30 pl-lg">
      <h3 className="text-label font-semibold text-muted-foreground">{SETUP_COPY.instructions}</h3>
      <FleetMarkdownDynamic>{fleet.definition.instructions_md}</FleetMarkdownDynamic>
    </Section>
    <TriggerPanel triggers={fleet.triggers} lastDeliveryByKey={lastDeliveryByKey} />
  </Section>;
}
