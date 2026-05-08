// error-map-presets — shared UZ-* → user-facing { code, message } maps.
// Per-command errorMaps in zombiectl/src/commands/*.js compose presets
// with command-specific entries. The audit (scripts/audit-cli-runcommand.sh)
// reads each command's exported errorMap and asserts coverage of the
// codes its endpoints can return.
//
// Stable surface: once a code → message lands here, treat the message
// string as a public string (RULE EMS). Refactors must keep the
// substring search in the failure-modes integration tests valid.

// Universal auth codes — every authenticated command can hit these.
// Mirrors src/errors/error_registry.zig UZ-AUTH-001..010.
export const AUTH_PRESET = Object.freeze({
  "UZ-AUTH-001": {
    code: "FORBIDDEN",
    message: "Access denied — your role does not permit this action.",
  },
  "UZ-AUTH-002": {
    code: "UNAUTHORIZED",
    message: "Not authenticated — run `zombiectl login` to start a session.",
  },
  "UZ-AUTH-003": {
    code: "TOKEN_EXPIRED",
    message: "Token expired — run `zombiectl login` to refresh.",
  },
  "UZ-AUTH-004": {
    code: "AUTH_UNAVAILABLE",
    message: "Authentication service unavailable — try again shortly.",
  },
  "UZ-AUTH-005": {
    code: "SESSION_NOT_FOUND",
    message: "Login session not found — start a fresh `zombiectl login`.",
  },
  "UZ-AUTH-006": {
    code: "SESSION_EXPIRED",
    message: "Login session expired — start a fresh `zombiectl login`.",
  },
  "UZ-AUTH-007": {
    code: "SESSION_ALREADY_COMPLETE",
    message: "Login session already completed — re-run `zombiectl login` if you need a new token.",
  },
  "UZ-AUTH-008": {
    code: "SESSION_LIMIT",
    message: "Too many active login sessions — wait a minute and retry.",
  },
  "UZ-AUTH-009": {
    code: "INSUFFICIENT_ROLE",
    message: "Your role does not have permission for this action.",
  },
  "UZ-AUTH-010": {
    code: "UNSUPPORTED_ROLE",
    message: "Server does not recognize your role — contact support.",
  },
});

// Workspace lifecycle codes that surface to the human operator.
export const WORKSPACE_PRESET = Object.freeze({
  "UZ-WORKSPACE-001": {
    code: "WORKSPACE_NOT_FOUND",
    message: "Workspace not found — check `zombiectl workspace list`.",
  },
  "UZ-WORKSPACE-002": {
    code: "WORKSPACE_PAUSED",
    message: "Workspace paused — resolve billing in the dashboard before continuing.",
  },
  "UZ-WORKSPACE-003": {
    code: "WORKSPACE_FREE_LIMIT",
    message: "Free-tier workspace limit reached — upgrade or reuse an existing workspace.",
  },
});

// Zombie lifecycle codes that surface from install / list / status etc.
export const ZOMBIE_PRESET = Object.freeze({
  "UZ-ZMB-006": {
    code: "ZOMBIE_NAME_EXISTS",
    message: "A zombie with this name already exists — pick a different name.",
  },
  "UZ-ZMB-008": {
    code: "ZOMBIE_INVALID_CONFIG",
    message: "Zombie config is invalid — check the SKILL.md and required fields.",
  },
  "UZ-ZMB-009": {
    code: "ZOMBIE_NOT_FOUND",
    message: "Zombie not found in this workspace.",
  },
  "UZ-ZMB-010": {
    code: "ZOMBIE_ALREADY_TERMINAL",
    message: "Zombie is already in a terminal state — kill/delete is final.",
  },
  "UZ-ZMB-011": {
    code: "ZOMBIE_NAME_MISMATCH",
    message: "Zombie name does not match — recheck the install path.",
  },
});

// Compose helper. Last argument wins, so per-command overrides at the
// call site shadow preset entries.
export function compose(...sources) {
  return Object.freeze(Object.assign({}, ...sources));
}
