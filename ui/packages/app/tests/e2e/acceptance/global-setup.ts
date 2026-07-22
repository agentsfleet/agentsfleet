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
 *      (the old public mailinator inbox made any password leak = direct PROD
 *      account access; fixtures now live on MX-less e2e.agentsfleet.net).
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
import { request as playwrightRequest } from "@playwright/test";
import { clerkSetup } from "@clerk/testing/playwright";
import {
  attachJwt,
  finalizeFixtureMetadata,
  provisionUser,
  type FixtureUserSpec,
  type MintedFixture,
} from "./fixtures/clerk-admin";
import { bootstrapTenant } from "./fixtures/bootstrap";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { ensureSecondWorkspace, getDefaultWorkspaceId } from "./fixtures/seed";
import {
  FIXTURE_KEY,
  SECOND_WORKSPACE_NAME,
  VERCEL_BYPASS_STATE_FILENAME,
} from "./fixtures/constants";
import { diagnoseApiError } from "./fixtures/preflight";
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
// from op:// vault items. Defaults live on e2e.agentsfleet.net — an owned,
// MX-less subdomain, undeliverable by construction. Nobody can receive a
// login code for these addresses, unlike the old public mailinator inbox.
const DEFAULT_REGULAR_EMAIL = "regular-fixture@e2e.agentsfleet.net";
const DEFAULT_ADMIN_EMAIL = "admin-fixture@e2e.agentsfleet.net";
const DEFAULT_OPERATOR_EMAIL = "operator-fixture@e2e.agentsfleet.net";

// Random per-create password. The harness never logs in via password;
// CLERK_SECRET_KEY admin API mints sessions directly. A stable password
// would only enable an attacker who learns it (via source, leaked logs,
// undeliverable fixture inbox) to sign in via Clerk's hosted UI. 32 random
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

// Trade the raw Vercel bypass secret for its derived short-lived cookie ONCE,
// before any traced browser context exists. Every context starts from this
// storage state, so retained failure traces never record the loaded secret —
// only the cookie Vercel minted from it. Without a secret (local webServer
// runs) the state is empty and harmless.
async function primeVercelBypassState(): Promise<void> {
  const storagePath = path.join(process.cwd(), VERCEL_BYPASS_STATE_FILENAME);
  const secret = process.env.VERCEL_BYPASS_SECRET;
  const baseUrl = process.env.BASE_URL;
  if (!secret || !baseUrl) {
    fs.writeFileSync(storagePath, JSON.stringify({ cookies: [], origins: [] }));
    fs.chmodSync(storagePath, 0o600);
    return;
  }
  const context = await playwrightRequest.newContext({
    extraHTTPHeaders: {
      "x-vercel-protection-bypass": secret,
      "x-vercel-set-bypass-cookie": "true",
    },
  });
  try {
    // No redirect-following: the bypass cookie arrives on the first response,
    // and a redirecting (or compromised) target must never receive the raw
    // secret header on a second host.
    await context.get(baseUrl, { maxRedirects: 0 });
    await context.storageState({ path: storagePath });
  } finally {
    await context.dispose();
  }
  fs.chmodSync(storagePath, 0o600);
}

export default async function globalSetup(): Promise<void> {
  for (const key of REQUIRED_ENV) {
    if (!process.env[key]) failLoud(key);
  }
  await primeVercelBypassState();
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
  // One unscoped sweep of the shared fixture workspace while no worker is
  // running yet. Per-spec cleanups are scoped to their own seed prefix (a
  // parallel worker must never delete a sibling's fleet mid-test), so this
  // is the only place leftovers from interrupted runs get cleared — without
  // it they accumulate past the wall's first page and exact-count specs
  // starve.
  // API failures here are environment problems, not product drift — surface
  // them as the typed redacted diagnosis (error code + recovery playbook)
  // instead of a raw wire error, and never echo response bodies.
  try {
    const regularWs = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const swept = await cleanWorkspaceFleets(FIXTURE_KEY.regular, regularWs);
    // Provision the shared secondary workspace before any worker exists —
    // parallel specs racing ensureSecondWorkspace's list-then-create would
    // otherwise collide on the (tenant, name) uniqueness during a first run.
    await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    console.log(
      `[e2e:auth] env present (api=${process.env.NEXT_PUBLIC_API_URL}); ` +
        `${fixtures.length} fixture users provisioned in Clerk + bootstrapped in agentsfleetd; ` +
        `JWTs cached to ${JWT_CACHE_PATH}; ` +
        `${swept} leftover fleet(s) swept from the shared workspace`,
    );
  } catch (error) {
    throw diagnoseApiError(error, "global setup");
  }
}
