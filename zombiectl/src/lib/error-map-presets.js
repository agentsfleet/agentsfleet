// error-map-presets — shared UZ-* → user-facing { code, message } maps.
// Per-command errorMaps in zombiectl/src/commands/*.js compose presets
// with command-specific entries. The registry unit test
// (zombiectl/test/registry.unit.test.js) pins the shape, UZ-* key
// format, and AUTH_PRESET coverage on auth-critical routes.
//
// Stable surface: once a code → message lands here, treat the message
// string as a public string (RULE EMS). Refactors must keep the
// substring search in the failure-modes integration tests valid.
//
// Keys reference named ERR_* constants from ../constants/error-codes.js
// (allowlisted by scripts/audit-error-codes.sh) — never raw "UZ-..."
// literals here, which would trip the raw-literal gate.

import {
  ERR_FORBIDDEN,
  ERR_UNAUTHORIZED,
  ERR_TOKEN_EXPIRED,
  ERR_AUTH_UNAVAILABLE,
  ERR_SESSION_NOT_FOUND,
  ERR_SESSION_EXPIRED,
  ERR_SESSION_ALREADY_COMPLETE,
  ERR_SESSION_LIMIT,
  ERR_INSUFFICIENT_ROLE,
  ERR_UNSUPPORTED_ROLE,
  ERR_WORKSPACE_NOT_FOUND,
  ERR_WORKSPACE_PAUSED,
  ERR_ZOMBIE_NAME_EXISTS,
  ERR_ZOMBIE_INVALID_CONFIG,
  ERR_ZOMBIE_NOT_FOUND,
  ERR_ZOMBIE_ALREADY_TERMINAL,
  ERR_ZOMBIE_NAME_MISMATCH,
} from "../constants/error-codes.ts";

// Universal auth codes — every authenticated command can hit these.
// Mirrors src/errors/error_registry.zig UZ-AUTH-001..010.
export const AUTH_PRESET = Object.freeze({
  [ERR_FORBIDDEN]: {
    code: "FORBIDDEN",
    message: "Access denied — your role does not permit this action.",
  },
  [ERR_UNAUTHORIZED]: {
    code: "UNAUTHORIZED",
    message: "Not authenticated — run `zombiectl login` to start a session.",
  },
  [ERR_TOKEN_EXPIRED]: {
    code: "TOKEN_EXPIRED",
    message: "Token expired — run `zombiectl login` to refresh.",
  },
  [ERR_AUTH_UNAVAILABLE]: {
    code: "AUTH_UNAVAILABLE",
    message: "Authentication service unavailable — try again shortly.",
  },
  [ERR_SESSION_NOT_FOUND]: {
    code: "SESSION_NOT_FOUND",
    message: "Login session not found — start a fresh `zombiectl login`.",
  },
  [ERR_SESSION_EXPIRED]: {
    code: "SESSION_EXPIRED",
    message: "Login session expired — start a fresh `zombiectl login`.",
  },
  [ERR_SESSION_ALREADY_COMPLETE]: {
    code: "SESSION_ALREADY_COMPLETE",
    message: "Login session already completed — re-run `zombiectl login` if you need a new token.",
  },
  [ERR_SESSION_LIMIT]: {
    code: "SESSION_LIMIT",
    message: "Too many active login sessions — wait a minute and retry.",
  },
  [ERR_INSUFFICIENT_ROLE]: {
    code: "INSUFFICIENT_ROLE",
    message: "Your role does not have permission for this action.",
  },
  [ERR_UNSUPPORTED_ROLE]: {
    code: "UNSUPPORTED_ROLE",
    message: "Server does not recognize your role — contact support.",
  },
});

// Workspace lifecycle codes that surface to the human operator.
export const WORKSPACE_PRESET = Object.freeze({
  [ERR_WORKSPACE_NOT_FOUND]: {
    code: "WORKSPACE_NOT_FOUND",
    message: "Workspace not found — check `zombiectl workspace list`.",
  },
  [ERR_WORKSPACE_PAUSED]: {
    code: "WORKSPACE_PAUSED",
    message: "Workspace paused — resolve billing in the dashboard before continuing.",
  },
});

// Zombie lifecycle codes that surface from install / list / status etc.
export const ZOMBIE_PRESET = Object.freeze({
  [ERR_ZOMBIE_NAME_EXISTS]: {
    code: "ZOMBIE_NAME_EXISTS",
    message: "A zombie with this name already exists — pick a different name.",
  },
  [ERR_ZOMBIE_INVALID_CONFIG]: {
    code: "ZOMBIE_INVALID_CONFIG",
    message: "Zombie config is invalid — check the SKILL.md and required fields.",
  },
  [ERR_ZOMBIE_NOT_FOUND]: {
    code: "ZOMBIE_NOT_FOUND",
    message: "Zombie not found in this workspace.",
  },
  [ERR_ZOMBIE_ALREADY_TERMINAL]: {
    code: "ZOMBIE_ALREADY_TERMINAL",
    message: "Zombie is already in a terminal state — kill/delete is final.",
  },
  [ERR_ZOMBIE_NAME_MISMATCH]: {
    code: "ZOMBIE_NAME_MISMATCH",
    message: "Zombie name does not match — recheck the install path.",
  },
});

// Compose helper. Last argument wins, so per-command overrides at the
// call site shadow preset entries.
export function compose(...sources) {
  return Object.freeze(Object.assign({}, ...sources));
}
