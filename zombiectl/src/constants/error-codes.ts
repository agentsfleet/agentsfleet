/**
 * Server-side UZ-* error codes the CLI narrows on. The Zig registry
 * (`src/errors/error_registry.zig`) is the source of truth; these
 * mirrors exist so `scripts/audit-error-codes.sh` accepts the literal
 * strings here (the file is on the raw-literal allowlist) and lets
 * CLI source files import them as named constants instead of duplicating
 * the literal at every call site.
 *
 * Only auth-related codes are mirrored — those are the ones the CLI
 * branches on (in `src/commands/auth.ts`) to surface re-auth prompts.
 * Other UZ-* codes flow through the Effect dispatcher's renderError as
 * opaque strings; mirroring them here would be premature allocation.
 */

export const ERR_FORBIDDEN = "UZ-AUTH-001";
export const ERR_UNAUTHORIZED = "UZ-AUTH-002";
export const ERR_TOKEN_EXPIRED = "UZ-AUTH-003";
