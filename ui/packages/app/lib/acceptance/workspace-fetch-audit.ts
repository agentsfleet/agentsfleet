export type WorkspaceFetchAuditSnapshot = {
  total: number;
  byPath: Record<string, number>;
};

/** What one settled request cost, per id-free route template. */
export type WorkspaceFetchTiming = {
  durationsMs: number[];
  attempts: number[];
};
export type WorkspaceFetchTimings = Record<string, WorkspaceFetchTiming>;

/** The route's body: counts (what was asked) beside timings (what it cost). */
export type WorkspaceFetchAuditPayload = WorkspaceFetchAuditSnapshot & {
  timingsByPath: WorkspaceFetchTimings;
};

/** Recorded per settled request, so a measurement can attribute a wait. */
export type AuditedOutcome = {
  /**
   * Wraps a caller's own attempt callback; never replaces it. Generic over the
   * callback's info type so this module stays free of the api layer's
   * `AttemptInfo` — the transport imports the audit, so the dependency must not
   * point back.
   */
  trackAttempts: <T extends { attempt: number } = { attempt: number }>(
    next?: (info: T) => void,
  ) => (info: T) => void;
  settle: () => void;
};

type WorkspaceFetchAuditState = WorkspaceFetchAuditSnapshot & {
  timingsByPath: WorkspaceFetchTimings;
};

const AUDIT_ENV_NAME = "AGENTSFLEET_E2E_AUDIT";
const AUDIT_ENABLED_VALUE = "1";
const STATE_KEY = "__agentsfleetWorkspaceFetchAudit";

export const WORKSPACE_LIST_PATH = "/v1/tenants/me/workspaces";
// Spelled here rather than imported from `lib/api/*`: the transport imports
// THIS module to record, so importing an api module back would close a cycle.
// The api modules are still the source of truth for the live paths —
// `lib/api/tenant_provider.ts` and `lib/api/runners.ts` respectively.
export const TENANT_PROVIDER_PATH = "/v1/tenants/me/provider";
export const FLEET_RUNNERS_PATH = "/v1/fleets/runners";

// Audited GETs, keyed by their id-free route template so assertions never
// depend on seeded identifiers. `fleetMessages` vs `fleetEventDetail` is the
// chat view's request-count acceptance: one thread read, zero per-turn reads.
export const AUDITED_PATH = {
  workspaceList: WORKSPACE_LIST_PATH,
  fleetMessages: "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages",
  fleetEventDetail: "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}",
  // The Secrets page's pair, awaited together in one `Promise.all`, and the
  // Runners list. Counted so a page's upstream reads are a number rather than
  // a reading of its source.
  workspaceSecrets: "/v1/workspaces/{workspace_id}/secrets",
  tenantProvider: TENANT_PROVIDER_PATH,
  fleetRunners: FLEET_RUNNERS_PATH,
} as const;

const FLEET_MESSAGES_PATTERN = /^\/v1\/workspaces\/[^/]+\/fleets\/[^/]+\/messages$/;
// The trailing segment excludes `stream` so the live tail (which never rides
// this client anyway) can't be mistaken for a per-turn detail read.
const FLEET_EVENT_DETAIL_PATTERN = /^\/v1\/workspaces\/[^/]+\/fleets\/[^/]+\/events\/(?!stream$)[^/]+$/;
// Anchored so the per-secret routes (`…/secrets/{name}`) never count as the
// list read the Secrets page issues.
const WORKSPACE_SECRETS_PATTERN = /^\/v1\/workspaces\/[^/]+\/secrets$/;
const QUERY_SEPARATOR = "?";

type GlobalWithAudit = typeof globalThis & {
  [STATE_KEY]?: WorkspaceFetchAuditState;
};

// A long lane would otherwise grow these arrays without bound. The cap is far
// above any declared sample count, so a measurement never silently loses one.
const MAX_TIMING_SAMPLES_PER_PATH = 200;
const INERT_OUTCOME: AuditedOutcome = {
  trackAttempts:
    <T extends { attempt: number }>(next?: (info: T) => void) =>
    (info: T) => next?.(info),
  settle: () => {},
};

function emptyState(): WorkspaceFetchAuditState {
  return { total: 0, byPath: {}, timingsByPath: {} };
}

function auditState(): WorkspaceFetchAuditState {
  const globalWithAudit = globalThis as GlobalWithAudit;
  globalWithAudit[STATE_KEY] ??= emptyState();
  return globalWithAudit[STATE_KEY];
}

export function isWorkspaceFetchAuditEnabled(): boolean {
  return typeof process !== "undefined" && process.env[AUDIT_ENV_NAME] === AUDIT_ENABLED_VALUE;
}

/** The template key a request path counts under, or null when unaudited.
 * The query string is stripped first — real calls carry `?limit=` etc. */
function auditedKeyFor(path: string): string | null {
  const separator = path.indexOf(QUERY_SEPARATOR);
  const bare = separator === -1 ? path : path.slice(0, separator);
  if (bare === WORKSPACE_LIST_PATH) return AUDITED_PATH.workspaceList;
  if (FLEET_MESSAGES_PATTERN.test(bare)) return AUDITED_PATH.fleetMessages;
  if (FLEET_EVENT_DETAIL_PATTERN.test(bare)) return AUDITED_PATH.fleetEventDetail;
  if (WORKSPACE_SECRETS_PATTERN.test(bare)) return AUDITED_PATH.workspaceSecrets;
  if (bare === TENANT_PROVIDER_PATH) return AUDITED_PATH.tenantProvider;
  if (bare === FLEET_RUNNERS_PATH) return AUDITED_PATH.fleetRunners;
  return null;
}

export function recordWorkspaceFetchForAcceptance(path: string): void {
  if (!isWorkspaceFetchAuditEnabled()) return;
  const key = auditedKeyFor(path);
  if (key === null) return;

  const state = auditState();
  state.total += 1;
  state.byPath[key] = (state.byPath[key] ?? 0) + 1;
}

export function readWorkspaceFetchAudit(): WorkspaceFetchAuditSnapshot {
  const state = auditState();
  return { total: state.total, byPath: { ...state.byPath } };
}

export function readWorkspaceFetchTimings(): WorkspaceFetchTimings {
  const state = auditState();
  return Object.fromEntries(
    Object.entries(state.timingsByPath).map(([key, timing]) => [
      key,
      { durationsMs: [...timing.durationsMs], attempts: [...timing.attempts] },
    ]),
  );
}

/** Counts and timings together — what the acceptance route serves. */
export function readWorkspaceFetchAuditPayload(): WorkspaceFetchAuditPayload {
  return { ...readWorkspaceFetchAudit(), timingsByPath: readWorkspaceFetchTimings() };
}

/**
 * Opens a timing record for one logical GET. `recordWorkspaceFetchForAcceptance`
 * counts the ASK; this records what the ask COST — wall time and the attempts
 * the retry ladder actually took — which is what attributes a slow render to a
 * stage rather than to a guess. Settle on the failure path too: a read that
 * exhausts the ladder is the case a latency investigation most needs.
 */
export function beginWorkspaceFetchOutcome(path: string): AuditedOutcome {
  if (!isWorkspaceFetchAuditEnabled()) return INERT_OUTCOME;
  const key = auditedKeyFor(path);
  if (key === null) return INERT_OUTCOME;

  const startedAt = Date.now();
  let attempts = 0;
  return {
    trackAttempts:
      <T extends { attempt: number }>(next?: (info: T) => void) =>
      (info: T) => {
        attempts = Math.max(attempts, info.attempt);
        next?.(info);
      },
    settle: () => {
      const state = auditState();
      state.timingsByPath[key] ??= { durationsMs: [], attempts: [] };
      const timing = state.timingsByPath[key];
      if (timing.durationsMs.length >= MAX_TIMING_SAMPLES_PER_PATH) return;
      timing.durationsMs.push(Date.now() - startedAt);
      timing.attempts.push(attempts);
    },
  };
}

export function resetWorkspaceFetchAudit(): WorkspaceFetchAuditSnapshot {
  const globalWithAudit = globalThis as GlobalWithAudit;
  globalWithAudit[STATE_KEY] = emptyState();
  return readWorkspaceFetchAudit();
}
