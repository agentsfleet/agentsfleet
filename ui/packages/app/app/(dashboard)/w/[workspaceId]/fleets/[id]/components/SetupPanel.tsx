import Link from "next/link";
import { Badge, Button, DashboardPanel, DashboardPanelTitle } from "@agentsfleet/design-system";
import { workspacePath } from "@/lib/workspace-routes";
import type { FleetDetail } from "@/lib/types";
import { FleetMarkdownDynamic } from "@/components/domain/FleetMarkdownDynamic";

export function SetupPanel({workspaceId, fleet}: {workspaceId: string; fleet: FleetDetail}) {
  const query = new URLSearchParams({fleet: fleet.id, setup_revision: fleet.setup_revision});
  return <DashboardPanel className="space-y-lg">
    <div className="flex flex-wrap items-center justify-between gap-md">
      <div className="space-y-xs"><DashboardPanelTitle>Fleet setup</DashboardPanelTitle>
        <p className="text-sm text-muted-foreground">Revision {fleet.setup_revision}. Running work keeps the revision it started with.</p></div>
      <Button asChild><Link href={`${workspacePath(workspaceId, "fleets/new")}?${query}`}>Improve in private chat</Link></Button>
    </div>
    {fleet.needs_re_review ? <Badge variant="amber">Review current access before resuming</Badge> : null}
    <div className="space-y-sm"><h2 className="font-medium">Instructions</h2><FleetMarkdownDynamic>{fleet.definition.instructions_md}</FleetMarkdownDynamic></div>
    <div className="space-y-sm"><h2 className="font-medium">Runtime configuration</h2>
      <pre className="max-h-96 overflow-auto rounded-md bg-muted p-md text-xs">{JSON.stringify(fleet.definition.config, null, 2)}</pre></div>
  </DashboardPanel>;
}
