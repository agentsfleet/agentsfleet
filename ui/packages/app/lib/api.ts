import type { Workspace, Run, RunTransition, Spec, PaginatedResponse } from "./types";

const BASE = process.env.NEXT_PUBLIC_API_URL ?? "https://api.usezombie.com";

async function request<T>(
  path: string,
  init?: RequestInit,
  token?: string,
): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: {
      "Content-Type": "application/json",
      ...(token ? { Authorization: `Bearer ${token}` } : {}),
      ...init?.headers,
    },
  });

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    throw Object.assign(new Error(body.error ?? res.statusText), {
      status: res.status,
      code: body.code,
    });
  }

  return res.json() as Promise<T>;
}

// ── Workspaces ──

export async function listWorkspaces(token: string): Promise<PaginatedResponse<Workspace>> {
  return request<PaginatedResponse<Workspace>>("/v1/workspaces", {}, token);
}

export async function getWorkspace(id: string, token: string): Promise<Workspace> {
  return request<Workspace>(`/v1/workspaces/${id}`, {}, token);
}

export async function pauseWorkspace(id: string, token: string): Promise<void> {
  return request<void>(`/v1/workspaces/${id}/pause`, { method: "POST" }, token);
}

export async function resumeWorkspace(id: string, token: string): Promise<void> {
  return request<void>(`/v1/workspaces/${id}/resume`, { method: "POST" }, token);
}

// ── Runs ──

export async function listRuns(
  workspaceId: string,
  token: string,
): Promise<PaginatedResponse<Run>> {
  return request<PaginatedResponse<Run>>(`/v1/workspaces/${workspaceId}/runs`, {}, token);
}

export async function getRun(id: string, token: string): Promise<Run> {
  return request<Run>(`/v1/runs/${id}`, {}, token);
}

export async function retryRun(id: string, token: string): Promise<Run> {
  return request<Run>(`/v1/runs/${id}/retry`, { method: "POST" }, token);
}

export async function listRunTransitions(id: string, token: string): Promise<RunTransition[]> {
  return request<RunTransition[]>(`/v1/runs/${id}/transitions`, {}, token);
}

// ── Specs ──

export async function listSpecs(workspaceId: string, token: string): Promise<PaginatedResponse<Spec>> {
  return request<PaginatedResponse<Spec>>(`/v1/workspaces/${workspaceId}/specs`, {}, token);
}

export async function syncSpecs(workspaceId: string, token: string): Promise<{ synced: number }> {
  return request<{ synced: number }>(
    `/v1/workspaces/${workspaceId}/specs/sync`,
    { method: "POST" },
    token,
  );
}
