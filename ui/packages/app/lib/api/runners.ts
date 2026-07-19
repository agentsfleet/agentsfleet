import { request } from "./client";

// host_id is free-form but bounded by the backend; deriving HOST_ID_REGEX from
// HOST_ID_MAX keeps the form in step with `register.zig`'s MAX_HOST_ID_LEN as a
// single source — the bound lives in exactly one place.
export const HOST_ID_MAX = 256;
export const HOST_ID_REGEX = new RegExp(`^[A-Za-z0-9_.-]{1,${HOST_ID_MAX}}$`);
export const LABEL_REGEX = /^[A-Za-z0-9_.-]{1,64}$/;

// Self-reported isolation strength — mirrors `protocol.SandboxTier` verbatim
// (UFS: the tag names are the wire shape). `dev_none` is dev-only; a release
// daemon refuses it at boot.
export const SANDBOX_TIERS = ["landlock_full", "container_nested", "macos_seatbelt", "dev_none"] as const;
export type SandboxTier = (typeof SANDBOX_TIERS)[number];

// Operator-facing labels for the self-reported isolation stack — the raw enum
// tags are the wire shape (sent verbatim), these are the human strings the
// dropdown and list render. Keyed so a new tier can't be added without a label.
export const SANDBOX_TIER_LABELS: Record<SandboxTier, string> = {
  landlock_full: "Linux · Landlock (full)",
  container_nested: "Nested container",
  macos_seatbelt: "macOS · Seatbelt",
  dev_none: "None (dev only)",
};

// One-line descriptions for the isolation-mode OptionCard picker. Keyed by
// the same SandboxTier so a new tier can't be added without one. Mirrors
// docs/architecture/runner_fleet.md §Sandbox tiers — do not restate from the
// label alone; that table is the source of truth for what each tier means.
export const SANDBOX_TIER_DESCRIPTIONS: Record<SandboxTier, string> = {
  landlock_full: "Bare Linux host with kernel-level Landlock sandboxing — full isolation, eligible for any work.",
  container_nested: "Runner runs inside a container on a Linux host or VM — same full-isolation tier as Landlock.",
  macos_seatbelt: "macOS's Seatbelt sandbox — weaker isolation; limited to your own workspace's dev work.",
  dev_none: "No real sandbox — for local development builds only; a non-debug runner build refuses to start with this tier.",
};

// Derived runtime liveness — mirrors `protocol.RunnerLiveness` tag names. Never
// stored; computed server-side from last_seen_at + the live-lease join.
export const RUNNER_LIVENESS = ["registered", "busy", "online", "offline"] as const;
export type RunnerLiveness = (typeof RUNNER_LIVENESS)[number];

export const RUNNER_ADMIN_STATE = {
  active: "active",
  cordoned: "cordoned",
  draining: "draining",
  drained: "drained",
  revoked: "revoked",
} as const;
export type RunnerAdminState = (typeof RUNNER_ADMIN_STATE)[keyof typeof RUNNER_ADMIN_STATE];
export const RUNNER_ADMIN_STATES = [
  RUNNER_ADMIN_STATE.active,
  RUNNER_ADMIN_STATE.cordoned,
  RUNNER_ADMIN_STATE.draining,
  RUNNER_ADMIN_STATE.drained,
  RUNNER_ADMIN_STATE.revoked,
] as const;

export const RUNNER_ADMIN_ACTION = {
  cordon: "cordon",
  drain: "drain",
  revoke: "revoke",
} as const;
export type RunnerAdminAction = (typeof RUNNER_ADMIN_ACTION)[keyof typeof RUNNER_ADMIN_ACTION];
export const RUNNER_ADMIN_ACTIONS = [
  RUNNER_ADMIN_ACTION.cordon,
  RUNNER_ADMIN_ACTION.drain,
  RUNNER_ADMIN_ACTION.revoke,
] as const;

export const RUNNER_EVENT_TYPES = [
  "runner_registered",
  "runner_online",
  "runner_offline",
  "lease_acquired",
  "lease_released",
  "runner_cordoned",
  "runner_draining",
  "runner_drained",
  "runner_revoked",
] as const;
export type RunnerEventType = (typeof RUNNER_EVENT_TYPES)[number];

export const RUNNER_SORTS = ["-created_at", "created_at", "host_id", "-host_id"] as const;
export type RunnerSort = (typeof RUNNER_SORTS)[number];

export const DEFAULT_PAGE_SIZE = 25;
export const DEFAULT_SORT: RunnerSort = "-created_at";
const FLEET_RUNNERS_PATH = "/v1/fleets/runners";
const RUNNERS_ENROLLMENT_PATH = "/v1/runners";

export interface RunnerListItem {
  id: string;
  host_id: string;
  sandbox_tier: SandboxTier;
  admin_state: RunnerAdminState;
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

/** The mint response — `runner_token` is the raw `agt_r`, returned exactly once. */
export interface CreatedRunner {
  runner_id: string;
  runner_token: string;
}

export interface RunnerAdminStateUpdate {
  id: string;
  admin_state: RunnerAdminState;
}

export interface RunnerEventItem {
  id: string;
  runner_id: string;
  event_type: RunnerEventType;
  occurred_at: number;
  metadata: unknown;
}

export interface RunnerEventsResponse {
  items: RunnerEventItem[];
  total: number;
  page: number;
  page_size: number;
}

export interface ListParams {
  page?: number;
  page_size?: number;
  sort?: RunnerSort;
}

export interface EventListParams {
  page?: number;
  page_size?: number;
  event_type?: RunnerEventType;
  since?: number;
  until?: number;
}

export async function listRunners(token: string, params: ListParams = {}): Promise<RunnerListResponse> {
  const qs = new URLSearchParams({
    page: String(params.page ?? 1),
    page_size: String(params.page_size ?? DEFAULT_PAGE_SIZE),
    sort: params.sort ?? DEFAULT_SORT,
  });
  return request<RunnerListResponse>(`${FLEET_RUNNERS_PATH}?${qs.toString()}`, { method: "GET" }, token);
}

export async function createRunner(
  token: string,
  body: { host_id: string; sandbox_tier: SandboxTier; labels: string[] },
): Promise<CreatedRunner> {
  return request<CreatedRunner>(RUNNERS_ENROLLMENT_PATH, { method: "POST", body: JSON.stringify(body) }, token);
}

export async function updateRunnerAdminState(
  token: string,
  runnerId: string,
  action: RunnerAdminAction,
): Promise<RunnerAdminStateUpdate> {
  return request<RunnerAdminStateUpdate>(
    `${FLEET_RUNNERS_PATH}/${runnerId}`,
    { method: "PATCH", body: JSON.stringify({ action }) },
    token,
  );
}

/** Retires a revoked runner's record. 409 UZ-RUN-016 if it is not revoked yet. */
export async function deleteRunner(token: string, runnerId: string): Promise<void> {
  await request<void>(`${FLEET_RUNNERS_PATH}/${runnerId}`, { method: "DELETE" }, token);
}

export async function listRunnerEvents(
  token: string,
  runnerId: string,
  params: EventListParams = {},
): Promise<RunnerEventsResponse> {
  const qs = new URLSearchParams({
    page: String(params.page ?? 1),
    page_size: String(params.page_size ?? DEFAULT_PAGE_SIZE),
  });
  if (params.event_type) qs.set("event_type", params.event_type);
  if (params.since !== undefined) qs.set("since", String(params.since));
  if (params.until !== undefined) qs.set("until", String(params.until));
  return request<RunnerEventsResponse>(
    `${FLEET_RUNNERS_PATH}/${runnerId}/events?${qs.toString()}`,
    { method: "GET" },
    token,
  );
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
