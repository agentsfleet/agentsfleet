import { request } from "./client";

export async function removeWorkspacePluginEntry(workspace: string, id: string, token: string): Promise<void> {
  const path = `/v1/workspaces/${encodeURIComponent(workspace)}/plugins/${encodeURIComponent(id)}`;
  await request<void>(path, {method: "DELETE"}, token);
}
