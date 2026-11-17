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
  /** Events and leases are operator history; a tenant's group omits them. */
  readonly history: boolean;
  /**
   * A tenant registration sends one Idempotency-Key per invocation and may
   * retry with it; the platform route takes no key, so it never retries.
   */
  readonly idempotentRegister: boolean;
};

export const PLATFORM_RUNNERS: RunnerKind = {
  path: ADMIN_RUNNERS_PATH,
  description: "Manage platform execution hosts",
  scopes: {read: "runner:read", write: "runner:write", register: "runner:register"},
  history: true,
  idempotentRegister: false,
};

export const TENANT_RUNNERS: RunnerKind = {
  path: TENANT_RUNNERS_PATH,
  description: "Manage your tenant's own runners, which take only its work",
  scopes: {read: "runner:tenant", write: "runner:tenant", register: "runner:tenant"},
  history: false,
  idempotentRegister: true,
};
