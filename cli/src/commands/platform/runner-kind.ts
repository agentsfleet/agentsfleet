import { ADMIN_RUNNERS_PATH } from "./common.ts";

/** The tenant's own runners. */
export const TENANT_RUNNERS_PATH = "/v1/tenants/me/runners";

/**
 * One kind of runner the CLI manages: its API path, the scope each verb needs,
 * and what only the platform offers. Both command groups are built from one of
 * these, so `platform runners` and `runners` share every handler.
 */
export type RunnerKind = {
  readonly path: string;
  readonly description: string;
  readonly scopes: {readonly read: string; readonly write: string; readonly register: string};
  /** History and runs are operator reads; a tenant's group omits them. */
  readonly history: boolean;
};

export const PLATFORM_RUNNERS: RunnerKind = {
  path: ADMIN_RUNNERS_PATH,
  description: "Manage the machines that run every account's fleets",
  scopes: {read: "runner:read", write: "runner:write", register: "runner:register"},
  history: true,
};

export const TENANT_RUNNERS: RunnerKind = {
  path: TENANT_RUNNERS_PATH,
  description: "Manage your own machines that run only your fleets",
  scopes: {read: "runner:tenant", write: "runner:tenant", register: "runner:tenant"},
  history: false,
};
