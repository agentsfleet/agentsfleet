export type WorkspaceFetchAuditSnapshot = {
  total: number;
  byPath: Record<string, number>;
};

type WorkspaceFetchAuditState = WorkspaceFetchAuditSnapshot;

const AUDIT_ENV_NAME = "AGENTSFLEET_E2E_AUDIT";
const AUDIT_ENABLED_VALUE = "1";
const STATE_KEY = "__agentsfleetWorkspaceFetchAudit";

export const WORKSPACE_LIST_PATH = "/v1/tenants/me/workspaces";

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

export function recordWorkspaceFetchForAcceptance(path: string): void {
  if (!isWorkspaceFetchAuditEnabled() || path !== WORKSPACE_LIST_PATH) return;

  const state = auditState();
  state.total += 1;
  state.byPath[path] = (state.byPath[path] ?? 0) + 1;
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
