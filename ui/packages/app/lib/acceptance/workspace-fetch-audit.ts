export type WorkspaceFetchAuditSnapshot = {
  total: number;
  byPath: Record<string, number>;
};

type WorkspaceFetchAuditState = WorkspaceFetchAuditSnapshot;

const AUDIT_ENV_NAME = "AGENTSFLEET_E2E_AUDIT";
const AUDIT_ENABLED_VALUE = "1";
const STATE_KEY = "__agentsfleetWorkspaceFetchAudit";

export const WORKSPACE_LIST_PATH = "/v1/tenants/me/workspaces";

// Audited GETs, keyed by their id-free route template so assertions never
// depend on seeded identifiers. `fleetMessages` vs `fleetEventDetail` is the
// chat view's request-count acceptance: one thread read, zero per-turn reads.
export const AUDITED_PATH = {
  workspaceList: WORKSPACE_LIST_PATH,
  fleetMessages: "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/messages",
  fleetEventDetail: "/v1/workspaces/{workspace_id}/fleets/{fleet_id}/events/{event_id}",
} as const;

const FLEET_MESSAGES_PATTERN = /^\/v1\/workspaces\/[^/]+\/fleets\/[^/]+\/messages$/;
// The trailing segment excludes `stream` so the live tail (which never rides
// this client anyway) can't be mistaken for a per-turn detail read.
const FLEET_EVENT_DETAIL_PATTERN = /^\/v1\/workspaces\/[^/]+\/fleets\/[^/]+\/events\/(?!stream$)[^/]+$/;
const QUERY_SEPARATOR = "?";

type GlobalWithAudit = typeof globalThis & {
  [STATE_KEY]?: WorkspaceFetchAuditState;
};

function emptyState(): WorkspaceFetchAuditState {
  return { total: 0, byPath: {} };
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

export function resetWorkspaceFetchAudit(): WorkspaceFetchAuditSnapshot {
  const globalWithAudit = globalThis as GlobalWithAudit;
  globalWithAudit[STATE_KEY] = emptyState();
  return readWorkspaceFetchAudit();
}
