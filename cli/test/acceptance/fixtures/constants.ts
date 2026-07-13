/**
 * Cross-runtime constants for the agentsfleet acceptance suite.
 *
 * RULE UFS: every literal that crosses a wire boundary or appears in ≥2
 * sites lives here once. Identifiers match the dashboard suite's
 * `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` verbatim
 * (same SCREAMING_SNAKE name, same value) so a rename surfaces on either
 * side instead of drifting silently.
 */

import crypto from "node:crypto";

export const CLERK_API_BASE = "https://api.clerk.com/v1";

export const JWT_TEMPLATE = "api";

export const IS_TEST_FIXTURE_METADATA_KEY = "is_test_fixture";

export const FIXTURE_EMAIL_VAULT_PATHS = {
  regular: "op://VAULT/e2e-fixtures-email/regular",
  admin: "op://VAULT/e2e-fixtures-email/admin",
} as const;

export const AGENTSFLEET_STATUS = {
  active: "active",
  paused: "paused",
  stopped: "stopped",
  killed: "killed",
  errored: "errored",
  terminated: "terminated",
} as const;

// `terminated` is a third post-kill resting state alongside `killed` /
// `errored`. The lifecycle suite accepts any of the three after
// `killFleet`. Omitting it would land terminated fleets in the
// teardown `live` list, where the kill retry trips UZ-AGT-010.
export const TERMINAL_STATUSES: ReadonlyArray<string> = [
  AGENTSFLEET_STATUS.killed,
  AGENTSFLEET_STATUS.errored,
  AGENTSFLEET_STATUS.terminated,
];

export const PLATFORM_OPS_SAMPLE_DIR = "tests/fixtures/fleetbundle/platform-ops";

// Matches the frontmatter `name:` the fixture bundle declares — which is also
// the slug of the agentsfleet/platform-ops repository and the fleet-library
// catalog id. The importer takes the catalog row id straight from this name, so
// the three must agree or an onboard mints a second entry instead of filling
// the seeded one. The acceptance helpers replace this exact name with a
// run-prefixed one so parallel runs don't collide on the uniqueness constraint.
export const PLATFORM_OPS_FIXTURE_NAME = "platform-ops";

export const LOGIN_POLL_MS = 500;

export const LOGIN_TIMEOUT_SEC = 60;

export const SESSION_TOKEN_TTL_SECONDS = 900;

export const FIXTURE_JWT_FILE = "test/acceptance/.fixture-jwt";

export const ACCEPTANCE_BINARY = {
  worktree: "worktree",
  global: "global",
} as const;

export const ACCEPTANCE_TARGET_ENV = "AGENTSFLEET_ACCEPTANCE_TARGET";

export const ACCEPTANCE_BINARY_ENV = "AGENTSFLEET_ACCEPTANCE_BINARY";

export const ACCEPTANCE_DASHBOARD_URL_ENV = "AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL";

export const UNROUTABLE_API_URL = "http://127.0.0.1:1";

// Per-environment API + dashboard URLs. Dashboard URLs derive from the
// acceptance API target — no separate skip-gating env var needed.
//   - PROD dashboard mirrors `runtime_loader.zig`'s `APP_URL` default.
//   - DEV dashboard is the Vercel-deploy URL used by the dev workflow
//     (per `playbooks/founding/04_deploy_dev/001_playbook.md` +
//     `playbooks/operations/credential_rotation/02_service_health.sh`).
export const API_URL_PROD = "https://api.agentsfleet.net";
export const API_URL_DEV = "https://api-dev.agentsfleet.net";
export const DASHBOARD_URL_PROD = "https://app.agentsfleet.net";
export const DASHBOARD_URL_DEV = "https://app-dev.agentsfleet.net";

// Per-process acceptance run identifier — every fleet created by this
// run is named `${ACCEPTANCE_RUN_PREFIX}-…`; every list/teardown
// assertion filters by it. Eliminates shared-DEV-tenant contention:
// the assertion becomes "no fleets from MY run remain" instead of
// "the tenant is globally empty" (which is never true).
export const ACCEPTANCE_RUN_PREFIX = `acc-${Date.now().toString(36)}-${crypto.randomBytes(3).toString("hex")}`;
