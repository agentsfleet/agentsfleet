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
// (docs/AUTH.md §Manually-provisioned). Two scopes only: the platform
// fleet-library scope the catalog specs exercise, and `runner:read` — the
// read-only runner-plane scope AUTH.md recommends for runner visibility —
// which the release preflight uses to prove a runner is online before the
// expensive journeys start. No write/admin runner scope: the preflight must
// stay incapable of mutating platform state by construction.
export const OPERATOR_FIXTURE_SCOPES = ["platform-library:write", "runner:read"] as const;

// Storage-state file global-setup primes with the short-lived Vercel bypass
// cookie. The browser then never sends the raw x-vercel-protection-bypass
// header, so retained failure traces record only the derived cookie — the
// loaded secret itself stays out of every uploaded artifact.
export const VERCEL_BYPASS_STATE_FILENAME = ".vercel-bypass-state.json";

// The regular fixture's second workspace, shared by every spec that exercises
// the WorkspaceSwitcher or workspace-scoped deep links. Provisioned ONCE in
// global-setup (before any worker exists) so parallel first runs can't race
// the list-then-create in ensureSecondWorkspace; specs only ever resolve it.
export const SECOND_WORKSPACE_NAME = "fixture-secondary";

/**
 * `@clerk/nextjs` major version that the harness was tested against. A bump
 * of this dependency's major is intentionally a breaking change: clerkMiddleware
 * may tighten cookie/JWT validation (e.g. start enforcing real dev-browser
 * tokens, rotate publicMetadata embedding), which would silently break or
 * silently relax the fixture wire. _smoke.spec.ts asserts the installed major
 * equals this constant so a `bun install` bump surfaces immediately.
 */
export const CLERK_NEXTJS_PINNED_MAJOR = 7;
