import { request } from "./client";

export type ActivityEvent = {
  id: string;
  zombie_id: string;
  workspace_id: string;
  event_type: string;
  detail: string;
  created_at: number;
};

export type ActivityPage = {
  events: ActivityEvent[];
  next_cursor: string | null;
};

export async function listWorkspaceActivity(
  workspaceId: string,
  token: string,
  cursor?: string,
): Promise<ActivityPage> {
  const params = cursor ? `?cursor=${encodeURIComponent(cursor)}` : "";
  return request<ActivityPage>(
    `/v1/workspaces/${workspaceId}/activity${params}`,
    { method: "GET" },
    token,
  );
}

export async function listZombieActivity(
  workspaceId: string,
  zombieId: string,
  token: string,
  cursor?: string,
): Promise<ActivityPage> {
  const params = cursor ? `?cursor=${encodeURIComponent(cursor)}` : "";
  return request<ActivityPage>(
    `/v1/workspaces/${workspaceId}/zombies/${zombieId}/activity${params}`,
    { method: "GET" },
    token,
  );
}
