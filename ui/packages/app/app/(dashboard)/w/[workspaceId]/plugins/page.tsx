import { requireCredential } from "@/lib/auth/credential";
import { PluginContent } from "./components/PluginContent";

export const dynamic = "force-dynamic";

export default async function PluginPage({params}: {params: Promise<{workspaceId: string}>}) {
  const {workspaceId} = await params;
  await requireCredential();
  return <PluginContent workspaceId={workspaceId} />;
}
