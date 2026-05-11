/**
 * Shared protocol constants for the auth e2e harness.
 *
 * Inline string literals like "active" / "paused" / "killed" / "regular" /
 * "admin" / "api" sit on cross-module wire boundaries (zombied state machine,
 * Clerk JWT template, fixture-pool keying). Centralising them surfaces a
 * rename as a type error rather than silent drift between seed/teardown/spec
 * code paths (RULE UFS).
 *
 * Canonical zombied state values live at
 * `ui/packages/app/lib/api/zombies.ts:ZOMBIE_STATUS`. The harness mirrors
 * them (lowercased keys so teardown / specs read naturally; values match
 * exactly).
 */

export const ZOMBIE_STATUS = {
  active: "active",
  paused: "paused",
  stopped: "stopped",
  killed: "killed",
  errored: "errored",
} as const;

export type ZombieStatus = (typeof ZOMBIE_STATUS)[keyof typeof ZOMBIE_STATUS];

export const FIXTURE_KEY = {
  regular: "regular",
  admin: "admin",
} as const;

export type FixtureKey = (typeof FIXTURE_KEY)[keyof typeof FIXTURE_KEY];

export const FIXTURE_KEYS: readonly FixtureKey[] = [
  FIXTURE_KEY.regular,
  FIXTURE_KEY.admin,
] as const;

/**
 * Clerk JWT template name. Mints a session JWT with publicMetadata
 * (tenant_id, role) embedded — the same template the dashboard consumes via
 * `getToken({ template: JWT_TEMPLATE })`. Default Clerk session tokens omit
 * publicMetadata, which would land zombied at 403 UZ-AUTH-001.
 */
export const JWT_TEMPLATE = "api";
