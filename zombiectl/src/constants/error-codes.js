/**
 * Server-side UZ-* error codes referenced from the CLI's errorMap
 * tables. The Zig registry (`src/errors/error_registry.zig`) is the
 * source of truth; identifiers here mirror Zig pub const names
 * verbatim (RULE UFS cross-runtime parity).
 *
 * The audit (`scripts/audit-error-codes.sh`) rejects raw `"UZ-..."`
 * literals outside this file — every CLI reference must resolve
 * through a named symbol here.
 *
 * Allowlisted in the audit (see `scripts/audit-error-codes.sh`).
 */

// Auth (UZ-AUTH-001 .. UZ-AUTH-010)
export const ERR_FORBIDDEN = "UZ-AUTH-001";
export const ERR_UNAUTHORIZED = "UZ-AUTH-002";
export const ERR_TOKEN_EXPIRED = "UZ-AUTH-003";
export const ERR_AUTH_UNAVAILABLE = "UZ-AUTH-004";
export const ERR_SESSION_NOT_FOUND = "UZ-AUTH-005";
export const ERR_SESSION_EXPIRED = "UZ-AUTH-006";
export const ERR_SESSION_ALREADY_COMPLETE = "UZ-AUTH-007";
export const ERR_SESSION_LIMIT = "UZ-AUTH-008";
export const ERR_INSUFFICIENT_ROLE = "UZ-AUTH-009";
export const ERR_UNSUPPORTED_ROLE = "UZ-AUTH-010";

// Billing
export const ERR_BILLING_UNAVAILABLE = "UZ-BILLING-001";
export const ERR_CREDIT_EXHAUSTED = "UZ-BILLING-005";

// Server-internal (database / runtime failures bubbling out as 5xx)
export const ERR_INTERNAL_DB_UNAVAILABLE = "UZ-INTERNAL-001";
export const ERR_INTERNAL_DB_QUERY = "UZ-INTERNAL-002";
export const ERR_INTERNAL_OPERATION_FAILED = "UZ-INTERNAL-003";

// Vault / credentials (workspace-scoped credential store)
export const ERR_VAULT_DATA_INVALID = "UZ-VAULT-001";

// Workspace lifecycle (UZ-WORKSPACE-001 .. UZ-WORKSPACE-003)
export const ERR_WORKSPACE_NOT_FOUND = "UZ-WORKSPACE-001";
export const ERR_WORKSPACE_PAUSED = "UZ-WORKSPACE-002";
export const ERR_WORKSPACE_FREE_LIMIT = "UZ-WORKSPACE-003";

// Zombie lifecycle (UZ-ZMB-006, 008..011)
export const ERR_ZOMBIE_NAME_EXISTS = "UZ-ZMB-006";
export const ERR_ZOMBIE_INVALID_CONFIG = "UZ-ZMB-008";
export const ERR_ZOMBIE_NOT_FOUND = "UZ-ZMB-009";
export const ERR_ZOMBIE_ALREADY_TERMINAL = "UZ-ZMB-010";
export const ERR_ZOMBIE_NAME_MISMATCH = "UZ-ZMB-011";

// Executor (runtime / nullclaw)
export const ERR_EXEC_RUNNER_AGENT_RUN = "UZ-EXEC-013";

// Integration grants
export const ERR_GRANT_NOT_FOUND = "UZ-GRANT-001";
export const ERR_GRANT_PENDING = "UZ-GRANT-002";
