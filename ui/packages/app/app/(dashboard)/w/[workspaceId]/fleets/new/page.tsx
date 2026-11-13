import { notFound } from "next/navigation";
import { z } from "zod";
import { PageLayout } from "@agentsfleet/design-system";
import { requireCredential } from "@/lib/auth/credential";
import { listTenantWorkspacesCached } from "@/lib/workspace";
import { WorkspaceChat } from "@/components/domain/chat/WorkspaceChat";
import { SOURCE_KIND } from "@/lib/api/draft-vocabulary";
import { PLUGIN_CREATE_PROMPT } from "@/components/domain/chat/chat-defaults";
import type { AuthoringIntent } from "@/components/domain/chat/AuthoringIntent";

export const dynamic = "force-dynamic";
const SearchSchema = z.union([
  z.object({draft: z.uuidv7()}).strict(),
  z.object({library: z.string().min(1).max(128), revision: z.uuidv7(), intent: z.enum(["edit", "new"]).optional()}).strict(),
  z.object({starter: z.literal("plugin")}).strict(),
  z.object({fleet: z.uuidv7(), setup_revision: z.string().regex(/^[1-9][0-9]*$/)}).strict(),
  z.object({}).strict(),
]);

export default async function NewFleetPage({params, searchParams}: {
  params: Promise<{workspaceId: string}>; searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const {workspaceId} = await params;
  const token = await requireCredential();
  const {items} = await listTenantWorkspacesCached(token);
  const workspace = items.find((item) => item.id === workspaceId);
  if (!workspace) notFound();
  const query = SearchSchema.safeParse(await searchParams);
  if (!query.success) notFound();
  const source = "library" in query.data ? {kind: SOURCE_KIND.library, library_id: query.data.library, revision_id: query.data.revision} : "fleet" in query.data ? {kind: SOURCE_KIND.fleet, fleet_id: query.data.fleet, setup_revision: query.data.setup_revision} : undefined;
  const intent = authoringIntent(query.data);
  return <PageLayout fullHeight className="flex min-h-0 flex-1 flex-col">
    <WorkspaceChat account={workspace.account.tenant_id} workspace={workspaceId} draft={"draft" in query.data ? query.data.draft : undefined} source={source}
      {...intent} />
  </PageLayout>;
}

function authoringIntent(query: z.infer<typeof SearchSchema>): AuthoringIntent {
  if ("library" in query) {
    return query.intent === "edit" ? {prompt: "Update this plugin’s setup.", publicationTarget: query.library}
      : {prompt: "Start a new fleet from this plugin. I want to review and adapt its setup."};
  }
  return "starter" in query ? {prompt: PLUGIN_CREATE_PROMPT} : {};
}
