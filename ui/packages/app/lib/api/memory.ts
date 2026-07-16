import { request } from "./client";
import type { MemoryEntry } from "../types";

export type { MemoryEntry };

// The tenant memory surface (M131 §5) — the console's first dashboard caller of
// the memory read (the CLI already lists memories). The read is `fleet:read`,
// limit-only, max 100; the forget is `fleet:write`.

export type MemoryListResponse = {
  items: MemoryEntry[];
  total: number;
  request_id: string;
};

// GET …/fleets/{id}/memories — what the fleet knows. `content` is the entry
// body (the column name is `content`, not `text`).
export async function listMemories(
  workspaceId: string,
  fleetId: string,
  token: string,
  opts?: { limit?: number },
): Promise<MemoryListResponse> {
  const qs = opts?.limit != null ? `?limit=${opts.limit}` : "";
  return request<MemoryListResponse>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/memories${qs}`,
    { method: "GET" },
    token,
  );
}

// DELETE …/fleets/{id}/memories/{key} — the operator's correction path when a
// fleet learned something wrong. 204 on success; a missing key throws an
// ApiError with status 404 (UZ-MEM-004) — a mistyped key is surfaced, not
// swallowed. The key is path-encoded.
export async function forgetMemory(
  workspaceId: string,
  fleetId: string,
  key: string,
  token: string,
): Promise<void> {
  await request<void>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/memories/${encodeURIComponent(key)}`,
    { method: "DELETE" },
    token,
  );
}
