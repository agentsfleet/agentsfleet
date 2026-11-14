import { Button, PageHeader, PageLayout, PageTitle } from "@agentsfleet/design-system";
import Link from "next/link";
import { PlusIcon, ArrowUpRightIcon } from "@phosphor-icons/react/ssr";
import { workspacePath } from "@/lib/workspace-routes";
import { PluginWorkspace } from "@/components/domain/chat/PluginWorkspace";
import { PLUGIN_CREATE_PROMPT } from "@/components/domain/chat/chat-defaults";

import { PLUGIN_PAGE_TITLE, PLUGIN_PAGE_DESCRIPTION } from "../copy";

export function PluginContent({workspaceId}: {workspaceId: string}) {
  return <PageLayout>
    <PageHeader description={<span className="whitespace-pre-line">{PLUGIN_PAGE_DESCRIPTION}</span>} actions={<Button asChild><Link href={workspacePath(workspaceId, "fleets/new")}><PlusIcon size={16} />Create in chat</Link></Button>}><PageTitle>{PLUGIN_PAGE_TITLE}</PageTitle></PageHeader>
    <PluginWorkspace workspace={workspaceId} />
    <div className="flex flex-wrap items-center justify-between gap-xl border-t border-border pt-xl">
      <div><p className="text-eyebrow text-text-dim">Create a reusable setup</p><p className="mt-sm text-description">“{PLUGIN_CREATE_PROMPT}”</p></div>
      <Button asChild variant="outline"><Link href={`${workspacePath(workspaceId, "fleets/new")}?starter=plugin`}>Use this prompt<ArrowUpRightIcon size={16} /></Link></Button>
    </div>
  </PageLayout>;
}
