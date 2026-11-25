import type { ThreadPage } from "@/lib/api/turns";
import { useEffect, useMemo, useState } from "react";
import { AppRouterContext } from "next/dist/shared/lib/app-router-context.shared-runtime";
import { PathnameContext, SearchParamsContext } from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import { PageLayout, TooltipProvider } from "@agentsfleet/design-system";
import { ShellFrame } from "@/components/layout/ShellFrame";
import { RouteLoading } from "@/components/layout/RouteLoading";
import { NewFleetChat } from "@/components/domain/fleet/NewFleetChat";
import { FleetWorkspace } from "@/components/domain/fleet/FleetWorkspace";
import { FleetListSections } from "@/components/domain/fleet/FleetListSections";
import { FleetNetworkSection } from "@/components/domain/network/FleetNetworkSection";
import { ChatView } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/ChatView";
import { FleetHeader } from "@/app/(dashboard)/w/[workspaceId]/fleets/[id]/components/FleetHeader";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { buildRunSummary } from "@/lib/turns/run-summary";
import { OWN_ACCOUNT } from "@/tests/helpers/workspace-fixtures";
import { FLEET_DETAIL, MODEL, WORKSPACE } from "@/tests/helpers/fleet-fixtures";
import { NEW_FLEET } from "./new-fleet-data";

const NEW_PATH = `/w/${WORKSPACE}/fleets/new`;
const FLEET_PATH = new RegExp(`^/w/${WORKSPACE}/fleets/([^/]+)$`);
const VIEWER = "fixture-owner";
const WORKSPACES = [{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}];

/**
 * New fleet and the fleet page it opens, as the app composes them, under a
 * router that moves the address bar the way Next's does. Only the API is the
 * probe server's: a create makes the scenario's fleet, and its thread read
 * answers that fleet's first message.
 */
export function NewFleetFixture() {
  const [path, setPath] = useState(NEW_PATH);
  const router = useMemo(() => {
    const go = (href: string) => {history.replaceState(null, "", `${href}${location.search}`); setPath(href);};
    return {bfcacheId: "new-fleet", back() {}, forward() {}, refresh() {}, push: go, replace: go, async prefetch() {}};
  }, []);
  const fleet = FLEET_PATH.exec(path)?.[1];
  return <AppRouterContext value={router}><PathnameContext value={path}><SearchParamsContext value={new URLSearchParams()}>
    <TooltipProvider><ShellFrame workspaces={WORKSPACES}>
      {fleet && fleet !== "new" ? <FleetRoute key={fleet} fleetId={fleet} />
        : <PageLayout fullHeight className="flex min-h-0 flex-1 flex-col"><NewFleetChat workspace={WORKSPACE} models={[MODEL]} /></PageLayout>}
    </ShellFrame></TooltipProvider>
  </SearchParamsContext></PathnameContext></AppRouterContext>;
}

/** The fleet page: its thread read first, as the server renders it, then the live chat over it. */
function FleetRoute({fleetId}: {fleetId: string}) {
  const [page, setPage] = useState<ThreadPage | null>(null);
  useEffect(() => {
    const owner = new AbortController();
    void fetch(`/live/v1/workspaces/${WORKSPACE}/fleets/${fleetId}/messages?limit=20`, {signal: owner.signal})
      .then(response => response.json()).then((value: ThreadPage) => setPage(value)).catch(() => undefined);
    return () => owner.abort();
  }, [fleetId]);
  if (!page) return <RouteLoading label="Loading fleet" />;
  const fleet = FleetDetailSchema.parse({...FLEET_DETAIL, id: fleetId, name: NEW_FLEET.name, pending_approvals: 0});
  return <div data-fleet-workspace className="flex h-full min-h-0 flex-1 flex-col overflow-clip">
    <FleetWorkspace account={OWN_ACCOUNT.account.tenant_id} workspace={WORKSPACE} fleet={fleet} models={[MODEL]} viewer={VIEWER}>
      <FleetHeader workspaceId={WORKSPACE} fleet={fleet} />
      <div className="page-entrance flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-clip">
        <ChatView workspaceId={WORKSPACE} fleetId={fleetId} schedules={fleet.schedules} senderLabel={fleet.name} modelLabel={MODEL.label}
          lists={<><FleetListSections /><FleetNetworkSection /></>} initial={page.items}
          initialSummary={buildRunSummary(fleet.status, page, fleet.pending_approvals)}
          approvalsHref={`/w/${WORKSPACE}/fleets/${fleetId}#fleet-approvals`} viewer={VIEWER} senderNames={[]} />
      </div>
    </FleetWorkspace>
  </div>;
}
