import { request } from "./client";

// host_id is free-form but bounded by the backend; deriving HOST_ID_REGEX from
// HOST_ID_MAX keeps the form in step with `register.zig`'s MAX_HOST_ID_LEN as a
// single source — the bound lives in exactly one place.
export const HOST_ID_MAX = 256;
export const HOST_ID_REGEX = new RegExp(`^[A-Za-z0-9_.-]{1,${HOST_ID_MAX}}$`);
export const LABEL_REGEX = /^[A-Za-z0-9_.-]{1,64}$/;

// Self-reported isolation strength — mirrors `protocol.SandboxTier` verbatim
// (UFS: the tag names are the wire contract). `dev_none` is dev-only; a release
// daemon refuses it at boot.
export const SANDBOX_TIERS = ["landlock_full", "container_nested", "macos_seatbelt", "dev_none"] as const;
export type SandboxTier = (typeof SANDBOX_TIERS)[number];

// Derived runtime liveness — mirrors `protocol.RunnerLiveness` tag names. Never
// stored; computed server-side from last_seen_at + the live-lease join.
export const RUNNER_LIVENESS = ["registered", "busy", "online", "offline"] as const;
export type RunnerLiveness = (typeof RUNNER_LIVENESS)[number];

export const RUNNER_SORTS = ["-created_at", "created_at", "host_id", "-host_id"] as const;
export type RunnerSort = (typeof RUNNER_SORTS)[number];

export const DEFAULT_PAGE_SIZE = 25;
export const DEFAULT_SORT: RunnerSort = "-created_at";

export interface RunnerListItem {
  id: string;
  host_id: string;
  sandbox_tier: SandboxTier;
  liveness: RunnerLiveness;
  labels: string[];
  last_seen_at: number;
  created_at: number;
}

export interface RunnerListResponse {
  items: RunnerListItem[];
  total: number;
  page: number;
  page_size: number;
}

/** The mint response — `runner_token` is the raw `zrn_`, returned exactly once. */
export interface CreatedRunner {
  runner_id: string;
  runner_token: string;
}

export interface ListParams {
  page?: number;
  page_size?: number;
  sort?: RunnerSort;
}

export async function listRunners(token: string, params: ListParams = {}): Promise<RunnerListResponse> {
  const qs = new URLSearchParams({
    page: String(params.page ?? 1),
    page_size: String(params.page_size ?? DEFAULT_PAGE_SIZE),
    sort: params.sort ?? DEFAULT_SORT,
  });
  return request<RunnerListResponse>(`/v1/fleet/runners?${qs.toString()}`, { method: "GET" }, token);
}

export async function createRunner(
  token: string,
  body: { host_id: string; sandbox_tier: SandboxTier; labels: string[] },
): Promise<CreatedRunner> {
  return request<CreatedRunner>(`/v1/runners`, { method: "POST", body: JSON.stringify(body) }, token);
}

/**
 * Split the free-form labels field (comma-separated) into a deduped, validated
 * set. Returns the first offending label as an error so the form can surface it;
 * an empty/whitespace-only input is a valid empty set.
 */
export function parseLabels(raw: string): { labels: string[]; error: string | null } {
  const parts = raw.split(",").map((s) => s.trim()).filter((s) => s.length > 0);
  const seen = new Set<string>();
  for (const p of parts) {
    if (!LABEL_REGEX.test(p)) {
      return { labels: [], error: `Label "${p}" must be 1–64 chars: letters, digits, dot, hyphen, underscore` };
    }
    seen.add(p);
  }
  return { labels: [...seen], error: null };
}
