/**
 * Authenticated e2e harness — global setup.
 *
 * Runs once per suite before any auth spec. Responsibilities:
 *   1. Fail fast if any required env var is missing, with a copy-paste op-read
 *      recipe in the error body.
 *   2. Resolve fixture identities. Emails come from env vars (op://-resolved
 *      in CI). Passwords are randomly generated per provision and never
 *      persisted — the harness mints sessions through Clerk's admin API
 *      (CLERK_SECRET_KEY-authenticated), not through the user-password flow,
 *      so a stable password buys nothing and surfaces a real attack vector
 *      (mailinator inbox is public; any leak of the password = direct PROD
 *      account access via Clerk's hosted sign-in page).
 *   3. Provision the fixture users in Clerk (idempotent on email) tagged
 *      with `is_test_fixture: true` metadata so prod ops dashboards can
 *      filter them out.
 *   4. Bootstrap each fixture user's tenant in agentsfleetd by Svix-signing a
 *      `user.created` payload and POSTing /v1/auth/identity-events/clerk —
 *      same path Clerk hits in production (renamed from /v1/webhooks/clerk
 *      in M68). Idempotent (replay returns created:false).
 *   5. Cache the minted JWTs to .fixture-jwts.json so signInAs(page, key)
 *      can mount the cookie without re-minting per spec. The cache is
 *      gitignored at the repo root and stays out of Playwright's
 *      report/results dirs.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import * as crypto from "node:crypto";
import { clerkSetup } from "@clerk/testing/playwright";
import {
  attachJwt,
  finalizeFixtureMetadata,
  provisionUser,
  type FixtureUserSpec,
  type MintedFixture,
} from "./fixtures/clerk-admin";
import { bootstrapTenant } from "./fixtures/bootstrap";
import { FIXTURE_KEY } from "./fixtures/constants";
import { loadWorktreeEnv } from "./fixtures/env-loader";

// Defensive: playwright.acceptance.config.ts loads worktree-root .env, but
// globalSetup is the actual fail-fast point for missing creds and should
// re-load idempotently in case it's invoked outside the standard config.
loadWorktreeEnv();

const REQUIRED_ENV = [
  "NEXT_PUBLIC_API_URL",
  "CLERK_SECRET_KEY",
  // clerkSetup() from @clerk/testing also requires the publishable key;
  // listing it here makes the failure mode explicit at our fail-loud check
  // instead of bubbling up from inside the @clerk/testing internals.
  "CLERK_PUBLISHABLE_KEY",
  "CLERK_WEBHOOK_SECRET",
] as const;

// Fixture emails — opt-in env override. The CI workflows resolve these
// from op:// vault items. Defaults remain the historical mailinator
// addresses for local DEV runs (where the public-inbox concern is
// acceptable: local agentsfleetd + DEV Clerk + DEV billing balance only).
const DEFAULT_REGULAR_EMAIL = "regular-fixture@mailinator.com";
const DEFAULT_ADMIN_EMAIL = "admin-fixture@mailinator.com";
const DEFAULT_OPERATOR_EMAIL = "operator-fixture@mailinator.com";

// Random per-create password. The harness never logs in via password;
// CLERK_SECRET_KEY admin API mints sessions directly. A stable password
// would only enable an attacker who learns it (via source, leaked logs,
// public mailinator inbox) to sign in via Clerk's hosted UI. 32 random
// bytes = 256 bits of entropy.
function freshPassword(): string {
  return crypto.randomBytes(32).toString("base64url");
}

function fixtureUsers(): FixtureUserSpec[] {
  return [
    {
      key: FIXTURE_KEY.regular,
      email: process.env.AUTH_E2E_REGULAR_EMAIL ?? DEFAULT_REGULAR_EMAIL,
      password: freshPassword(),
    },
    {
      key: FIXTURE_KEY.admin,
      email: process.env.AUTH_E2E_ADMIN_EMAIL ?? DEFAULT_ADMIN_EMAIL,
      password: freshPassword(),
    },
    {
      key: FIXTURE_KEY.operator,
      email: process.env.AUTH_E2E_OPERATOR_EMAIL ?? DEFAULT_OPERATOR_EMAIL,
      password: freshPassword(),
    },
  ];
}

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

function failLoud(missing: string): never {
  throw new Error(
    `[e2e:auth] refusing to start: missing required env var ${missing}\n` +
      `Set in the workflow / shell before running:\n` +
      `  NEXT_PUBLIC_API_URL=https://api-dev.agentsfleet.net   # or other safe target\n` +
      `  CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')\n` +
      `  CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret')\n`,
  );
}

function writeCache(fixtures: MintedFixture[]): void {
  // password is intentionally NOT persisted — it was a per-create random
  // string used only for Clerk's createUser body, and the harness has no
  // downstream use for it. Persisting it would re-introduce the attack
  // surface this rewrite eliminates.
  const cache: Record<string, Omit<MintedFixture, "key" | "password">> = {};
  for (const f of fixtures) {
    cache[f.key] = {
      email: f.email,
      clerkUserId: f.clerkUserId,
      sessionId: f.sessionId,
      sessionJwt: f.sessionJwt,
    };
  }
  fs.writeFileSync(JWT_CACHE_PATH, JSON.stringify(cache, null, 2));
  // chmod unconditionally — writeFileSync's `mode` option only applies on
  // file creation, so a re-run over an existing world-readable file would
  // leave the loose perms in place.
  fs.chmodSync(JWT_CACHE_PATH, 0o600);
}

export default async function globalSetup(): Promise<void> {
  for (const key of REQUIRED_ENV) {
    if (!process.env[key]) failLoud(key);
  }
  await clerkSetup();
  // Ordered setup keeps JWT claims fresh:
  //   1. provisionUser: ensure each Clerk user exists (no JWT yet).
  //      Tags new users with publicMetadata.is_test_fixture=true so
  //      prod ops can filter them.
  //   2. bootstrapTenant: agentsfleetd creates tenant + writes tenant_id/role
  //      back to Clerk publicMetadata.
  //   3. finalizeFixtureMetadata: wait for tenant writeback, then append the
  //      operator-only scope without dropping ordinary tenant grants.
  //   4. attachJwt: mint session JWT — now the JWT snapshots the updated
  //      publicMetadata, so agentsfleetd API calls that require tenant context
  //      succeed.
  const users = fixtureUsers();
  const provisioned = await Promise.all(users.map(provisionUser));
  for (const user of provisioned) {
    await bootstrapTenant(user);
    await finalizeFixtureMetadata(user);
  }
  const fixtures: MintedFixture[] = [];
  for (const user of provisioned) {
    fixtures.push(await attachJwt(user));
  }
  writeCache(fixtures);
  console.log(
    `[e2e:auth] env present (api=${process.env.NEXT_PUBLIC_API_URL}); ` +
      `${fixtures.length} fixture users provisioned in Clerk + bootstrapped in agentsfleetd; ` +
      `JWTs cached to ${JWT_CACHE_PATH}`,
  );
}
