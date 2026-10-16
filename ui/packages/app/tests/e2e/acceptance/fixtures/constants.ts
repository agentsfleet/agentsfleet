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

import * as path from "node:path";

// Matches afr_tools/src/catalog/mod.rs:HTTP_REQUEST so fixture bundles pass native admission.
export const HTTP_REQUEST = "http_request";

// Where global setup caches the minted fixture JWTs. It must sit outside every
// folder Playwright uploads as a CI artifact, or live session tokens ride along.
export const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

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
  // Platform operator — carries read-only runner visibility plus library writes.
  // Kept separate from the tenant owner so hidden-platform assertions stay meaningful.
  operator: "operator",
  policyEditor: "policy-editor",
} as const;

export type FixtureKey = (typeof FIXTURE_KEY)[keyof typeof FIXTURE_KEY];

export const FIXTURE_KEYS: readonly FixtureKey[] = [
  FIXTURE_KEY.regular,
  FIXTURE_KEY.admin,
  FIXTURE_KEY.operator,
  FIXTURE_KEY.policyEditor,
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

// Runner mutation journeys enroll isolated, never-started hosts and retire them
// afterwards. The separate release-preflight operator stays read-only.
export const POLICY_EDITOR_FIXTURE_SCOPES = ["runner:write", "runner:enroll"] as const;
export const PLATFORM_FIXTURE_SCOPES: Partial<Record<FixtureKey, readonly string[]>> = {
  [FIXTURE_KEY.operator]: OPERATOR_FIXTURE_SCOPES,
  [FIXTURE_KEY.policyEditor]: POLICY_EDITOR_FIXTURE_SCOPES,
};

// Storage-state file global-setup primes with the short-lived Vercel bypass
// cookie. The browser then never sends the raw x-vercel-protection-bypass
// header, so retained failure traces record only the derived cookie — the
// loaded secret itself stays out of every uploaded artifact.
export const VERCEL_BYPASS_STATE_FILENAME = ".vercel-bypass-state.json";

// Resend's test inbox takes an invite email without delivering it; `+clerk_test` is Clerk's test-mode subaddress.
export const TEST_INBOX_LOCAL_PREFIX = "delivered+clerk_test_";
export const TEST_INBOX_DOMAIN = "resend.dev";

// The regular fixture's second workspace, shared by every spec that exercises
// the WorkspaceSwitcher or workspace-scoped deep links. Provisioned ONCE in
// global-setup (before any worker exists) so parallel first runs can't race
// the list-then-create in ensureSecondWorkspace; specs only ever resolve it.
export const SECOND_WORKSPACE_NAME = "fixture-secondary";

// The regular fixture's workspace for the Fleet library layout specs, and theirs
// alone: workspace-library.spec.ts clears every library entry in the default
// workspace after each test, which a layout spec on another worker would read
// as an empty page. Provisioned in global-setup for the reason given above.
export const LIBRARY_LAYOUT_WORKSPACE_NAME = "fixture-library-layout";

// Tags a test that times the page's own frames. A second worker streaming
// beside it on a shared runner doubles the frame time it measures, so the
// suite runs every test carrying this tag alone, after the parallel groups.
export const FRAME_BUDGET_TAG = "@frame-budget";

