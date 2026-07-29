import { request } from "./client";
import { walkList } from "./list-walk";
import { QUERY_STARTING_AFTER } from "./runners";
import type { MemoryEntry } from "../types";

export type { MemoryEntry };

// The tenant memory surface — the console's dashboard caller of the memory
// read (the CLI also lists memories). The read is `fleet:read` and pages by
// keyset (`starting_after` → `next_cursor`, newest-created first); the forget
// is `fleet:write`.

export type MemoryListResponse = {
  items: MemoryEntry[];
  total: number;
  next_cursor: string | null;
};

// GET …/fleets/{id}/memories — one bounded page of what the fleet knows.
// `content` is the entry body (the column name is `content`, not `text`).
export async function listMemories(
  workspaceId: string,
  fleetId: string,
  token: string,
  opts?: { limit?: number; starting_after?: string },
): Promise<MemoryListResponse> {
  const qs = new URLSearchParams();
  if (opts?.limit != null) qs.set("limit", String(opts.limit));
  if (opts?.starting_after != null) qs.set(QUERY_STARTING_AFTER, opts.starting_after);
  const q = qs.toString();
  return request<MemoryListResponse>(
    `/v1/workspaces/${workspaceId}/fleets/${fleetId}/memories${q ? `?${q}` : ""}`,
    { method: "GET" },
    token,
  );
}

// The memory panel renders the fleet's whole memory, not the first bounded
// read — memory entries accumulate per execution, so one page can silently
// truncate. Walks next_cursor to the end via the shared list-walk bound.
export async function listAllMemories(
  workspaceId: string,
  fleetId: string,
  token: string,
): Promise<{ items: MemoryEntry[] }> {
  const walked = await walkList<MemoryEntry>("memory list", (cursor) =>
    listMemories(workspaceId, fleetId, token, cursor !== null ? { starting_after: cursor } : undefined),
  );
  return { items: walked.items };
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
