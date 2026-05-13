/**
 * Server-side UZ-* error codes referenced from the CLI's errorMap
 * tables. The Zig registry (`src/errors/error_registry.zig`) is the
 * source of truth; these mirrors exist because the audit
 * (`scripts/audit-error-codes.sh`) rejects raw `"UZ-..."` literals
 * outside the allowlist — every CLI reference must resolve through
 * a named symbol here.
 *
 * Allowlisted in the audit (see `scripts/audit-error-codes.sh`).
 */

// Billing
export const ERR_BILLING_UNAVAILABLE = "UZ-BILLING-001";
export const ERR_BILLING_CREDIT_EXHAUSTED = "UZ-BILLING-005";

// Server-internal (database / runtime failures bubbling out as 5xx)
export const ERR_INTERNAL_DB_UNAVAILABLE = "UZ-INTERNAL-001";
export const ERR_INTERNAL_GENERIC = "UZ-INTERNAL-002";
export const ERR_INTERNAL_SERVER_ERROR = "UZ-INTERNAL-003";

// Vault / credentials (workspace-scoped credential store)
export const ERR_VAULT_INVALID = "UZ-VAULT-001";
export const ERR_CREDENTIAL_NOT_FOUND = "UZ-CRED-001";
export const ERR_CREDENTIAL_NAME_INVALID = "UZ-CRED-003";

// Executor (runtime / nullclaw)
export const ERR_ZOMBIE_RUNNER_FAILED = "UZ-EXEC-013";

// Integration grants
export const ERR_GRANT_NOT_FOUND = "UZ-GRANT-001";
export const ERR_GRANT_PENDING = "UZ-GRANT-002";
export const ERR_GRANT_REVOKED = "UZ-GRANT-003";
