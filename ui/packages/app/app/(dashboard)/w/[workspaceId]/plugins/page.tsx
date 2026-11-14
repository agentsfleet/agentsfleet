import { requireCredential } from "@/lib/auth/credential";
import { PluginContent } from "./components/PluginContent";
import {notFound} from "next/navigation";
import {pluginsUiEnabled} from "@/lib/plugin-ui";

export const dynamic = "force-dynamic";

export default async function PluginPage({params}: {params: Promise<{workspaceId: string}>}) {
  if (!pluginsUiEnabled()) notFound();
  const {workspaceId} = await params;
  await requireCredential();
  return <PluginContent workspaceId={workspaceId} />;
}
