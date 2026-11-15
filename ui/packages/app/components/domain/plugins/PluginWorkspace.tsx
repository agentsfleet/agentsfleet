"use client";

import { useRouter } from "next/navigation";
import { workspacePath } from "@/lib/workspace-routes";
import { PluginBrowser } from "./PluginBrowser";

export function PluginWorkspace({ workspace }: { workspace: string }) {
  const router = useRouter();
  return <PluginBrowser workspace={workspace} onSelect={(source, intent) => {
    const query = new URLSearchParams({ plugin: source.plugin_id, revision: source.revision_id, intent });
    router.push(`${workspacePath(workspace, "fleets/new")}?${query}`);
  }} />;
}
