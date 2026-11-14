import { PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import { PluginWorkspace } from "@/components/domain/plugins/PluginWorkspace";

import { PLUGIN_PAGE_TITLE, PLUGIN_PAGE_DESCRIPTION } from "../copy";

export function PluginContent({workspaceId}: {workspaceId: string}) {
  return <PageLayout>
    <PageHeader description={PLUGIN_PAGE_DESCRIPTION}><PageTitle>{PLUGIN_PAGE_TITLE}</PageTitle></PageHeader>
    <PluginWorkspace workspace={workspaceId} />
  </PageLayout>;
}
