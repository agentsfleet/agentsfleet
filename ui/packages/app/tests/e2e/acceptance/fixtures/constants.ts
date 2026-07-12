/**
 * Shared protocol constants for the auth e2e harness.
 *
 * Inline string literals like "active" / "paused" / "killed" / "regular" /
 * "admin" / "api" sit on cross-module wire boundaries (agentsfleetd state machine,
 * Clerk JWT template, fixture-pool keying). Centralising them surfaces a
 * rename as a type error rather than silent drift between seed/teardown/spec
 * code paths (RULE UFS).
 *
 * Canonical agentsfleetd state values live at
 * `ui/packages/app/lib/api/fleets.ts:AGENTSFLEET_STATUS`. The harness mirrors
 * them (lowercased keys so teardown / specs read naturally; values match
 * exactly).
 */

export const AGENTSFLEET_STATUS = {
  active: "active",
  paused: "paused",
  stopped: "stopped",
  killed: "killed",
  errored: "errored",
} as const;

export type FleetStatus = (typeof AGENTSFLEET_STATUS)[keyof typeof AGENTSFLEET_STATUS];

export const FIXTURE_KEY = {
  regular: "regular",
  admin: "admin",
  // Platform operator — the only fixture whose Clerk `public_metadata.scopes`
  // carries an operator scope. Kept separate from `admin` (which is a tenant
  // owner, not a platform operator) so the other fixtures stay scope-free and
  // the specs that assert an operator surface is HIDDEN keep their meaning.
  operator: "operator",
} as const;

export type FixtureKey = (typeof FIXTURE_KEY)[keyof typeof FIXTURE_KEY];

export const FIXTURE_KEYS: readonly FixtureKey[] = [
  FIXTURE_KEY.regular,
  FIXTURE_KEY.admin,
  FIXTURE_KEY.operator,
] as const;

// Provisioned onto the operator fixture's Clerk `public_metadata.scopes`, which
// the session-token template projects to the top-level `scopes` claim that both
// the dashboard's `readSessionScopes` and agentsfleetd's `requireScope` read
// (docs/AUTH.md §Manually-provisioned). Only the platform fleet-library scope is
// granted — the acceptance suite exercises that surface and nothing else.
export const OPERATOR_FIXTURE_SCOPES = ["platform-library:write"] as const;

/**
 * `@clerk/nextjs` major version that the harness was tested against. A bump
 * of this dependency's major is intentionally a breaking change: clerkMiddleware
 * may tighten cookie/JWT validation (e.g. start enforcing real dev-browser
 * tokens, rotate publicMetadata embedding), which would silently break or
 * silently relax the fixture wire. _smoke.spec.ts asserts the installed major
 * equals this constant so a `bun install` bump surfaces immediately.
 */
export const CLERK_NEXTJS_PINNED_MAJOR = 7;
