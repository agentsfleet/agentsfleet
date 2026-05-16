/**
 * Cross-runtime constants for the zombiectl acceptance suite.
 *
 * RULE UFS: every literal that crosses a wire boundary or appears in ≥2
 * sites lives here once. Identifiers match the dashboard suite's
 * `ui/packages/app/tests/e2e/acceptance/fixtures/constants.ts` verbatim
 * (same SCREAMING_SNAKE name, same value) so a rename surfaces on either
 * side instead of drifting silently.
 */

export const CLERK_API_BASE = "https://api.clerk.com/v1";

export const JWT_TEMPLATE = "api";

export const IS_TEST_FIXTURE_METADATA_KEY = "is_test_fixture";

export const FIXTURE_EMAIL_VAULT_PATHS = {
  regular: "op://VAULT/e2e-fixtures/regular/email",
  admin: "op://VAULT/e2e-fixtures/admin/email",
} as const;

export const ZOMBIE_STATUS = {
  active: "active",
  paused: "paused",
  stopped: "stopped",
  killed: "killed",
  errored: "errored",
  terminated: "terminated",
} as const;

// `terminated` is a third post-kill resting state alongside `killed` /
// `errored` — lifecycle-with-token.spec.ts §4a accepts any of the three
// after `killZombie`. Omitting it would land terminated zombies in the
// teardown `live` list, where the kill retry trips UZ-ZMB-010.
export const TERMINAL_STATUSES: ReadonlyArray<string> = [
  ZOMBIE_STATUS.killed,
  ZOMBIE_STATUS.errored,
  ZOMBIE_STATUS.terminated,
];

export const PLATFORM_OPS_SAMPLE_DIR = "samples/platform-ops";

export const LOGIN_POLL_MS = 500;

export const LOGIN_TIMEOUT_SEC = 60;

export const SESSION_TOKEN_TTL_SECONDS = 900;

export const FIXTURE_JWT_FILE = "test/acceptance/.fixture-jwt";

export const ACCEPTANCE_BINARY = {
  worktree: "worktree",
  global: "global",
} as const;

export const ACCEPTANCE_TARGET_ENV = "ZOMBIE_ACCEPTANCE_TARGET";

export const ACCEPTANCE_BINARY_ENV = "ZOMBIE_ACCEPTANCE_BINARY";

export const UNROUTABLE_API_URL = "http://127.0.0.1:1";
