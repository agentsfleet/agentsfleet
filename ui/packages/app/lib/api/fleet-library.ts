import { request } from "./client";

export async function removeWorkspaceLibraryEntry(workspace: string, id: string, token: string): Promise<void> {
  const path = `/v1/workspaces/${encodeURIComponent(workspace)}/fleet-libraries/${encodeURIComponent(id)}`;
  await request<void>(path, {method: "DELETE"}, token);
}
