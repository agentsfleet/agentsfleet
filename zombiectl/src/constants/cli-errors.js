/**
 * CLI-side error codes — emitted by handlers before any server call,
 * or when mapping server-side responses to a stable analytics dimension.
 *
 * Pairs with the server's UZ-* registry (src/errors/error_registry.zig).
 * Server-side codes flow through `displayCode` in run-command.js and
 * appear verbatim on stderr / JSON envelope `.error.code`; these are
 * for client-side rejections (validation, missing args, local-only
 * "not found" cases).
 *
 * RULE UFS — every error-code string in CLI source reads from this file.
 */

export const VALIDATION_ERROR = "VALIDATION_ERROR";
export const MISSING_ARGUMENT = "MISSING_ARGUMENT";
export const UNKNOWN_ARGUMENT = "UNKNOWN_ARGUMENT";
export const UNKNOWN_COMMAND = "UNKNOWN_COMMAND";
export const USAGE_ERROR = "USAGE_ERROR";
export const NO_WORKSPACE = "NO_WORKSPACE";
export const UNKNOWN_WORKSPACE = "UNKNOWN_WORKSPACE";
export const AUTH_REQUIRED = "AUTH_REQUIRED";
export const IO_ERROR = "IO_ERROR";
export const UNEXPECTED = "UNEXPECTED";
