import {useState} from "react";
import {AppRouterContext} from "next/dist/shared/lib/app-router-context.shared-runtime";
import {PathnameContext, SearchParamsContext} from "next/dist/shared/lib/hooks-client-context.shared-runtime";
import {Button, PageHeader, PageLayout, PageTitle, TooltipProvider} from "@agentsfleet/design-system";
import {ShellFrame} from "@/components/layout/ShellFrame";
import {MembersView} from "@/app/(dashboard)/settings/members/components/MembersView";
import ModelsRegistryTable from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelsRegistryTable";
import {ModelCatalogueProvider} from "@/app/(dashboard)/w/[workspaceId]/settings/models/components/ModelCatalogueProvider";
import {MODELS_PAGE_DESCRIPTION, MODELS_PAGE_TITLE} from "@/app/(dashboard)/w/[workspaceId]/settings/models/copy";
import {OWN_ACCOUNT} from "@/tests/helpers/workspace-fixtures";
import {WORKSPACE} from "@/tests/helpers/draft-fixtures";
import {SAMPLE_INVITES, SAMPLE_MEMBERS, SAMPLE_MODELS} from "./resource-settings-actions";

const router = {bfcacheId: "resources-review", back() {}, forward() {}, refresh() {}, push() {}, replace() {}, async prefetch() {}};
export function ResourceSettingsFixture() {
  const [team, setTeam] = useState(false);
  const workspaces = [{id: WORKSPACE, name: "Release workspace", created_at: "2026-01-01T00:00:00Z", ...OWN_ACCOUNT}];
  return <AppRouterContext value={router}><PathnameContext.Provider value={team ? "/settings/members" : `/w/${WORKSPACE}/settings/models`}>
    <SearchParamsContext.Provider value={new URLSearchParams()}><TooltipProvider>
      <style>{".app-shell{top:44px;height:calc(100dvh - 44px)}"}</style>
      <div className="fixed inset-x-0 top-0 z-50 flex h-11 items-center gap-sm overflow-x-auto whitespace-nowrap bg-surface-deep px-lg">
        <span className="mr-auto text-eyebrow text-muted-foreground">Component review · Sample data · No live writes</span>
        <Button size="sm" variant="ghost" onClick={() => setTeam(false)}>Models</Button>
        <Button size="sm" variant="ghost" onClick={() => setTeam(true)}>Team</Button>
      </div>
      <ShellFrame workspaces={workspaces}>{team ? <MembersView initialMembers={SAMPLE_MEMBERS} initialInvites={SAMPLE_INVITES} /> :
        <ModelCatalogueProvider><PageLayout fullHeight className="h-full overflow-hidden">
          <PageHeader description={MODELS_PAGE_DESCRIPTION}><PageTitle>{MODELS_PAGE_TITLE}</PageTitle></PageHeader>
          <ModelsRegistryTable workspaceId={WORKSPACE} workspaces={workspaces} initialPage={SAMPLE_MODELS} initialError={null} />
        </PageLayout></ModelCatalogueProvider>}
      </ShellFrame>
    </TooltipProvider></SearchParamsContext.Provider>
  </PathnameContext.Provider></AppRouterContext>;
}
