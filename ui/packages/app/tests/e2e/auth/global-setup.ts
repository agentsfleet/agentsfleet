/**
 * Authenticated e2e harness — global setup.
 *
 * Runs once per suite before any auth spec. Responsibilities:
 *   1. Fail fast if any required env var is missing, with a copy-paste op-read
 *      recipe in the error body.
 *   2. Provision two fixture users in Clerk (idempotent on email) and mint a
 *      session JWT for each.
 *   3. Bootstrap each fixture user's tenant in zombied by Svix-signing a
 *      `user.created` payload and POSTing /v1/webhooks/clerk — same path
 *      Clerk hits in production. Idempotent (replay returns created:false).
 *   4. Cache the minted JWTs to .fixture-jwts.json so signInAs(page, key)
 *      can mount the cookie without re-minting per spec.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { clerkSetup } from "@clerk/testing/playwright";
import {
  attachJwt,
  provisionUser,
  type FixtureUserSpec,
  type MintedFixture,
} from "./fixtures/clerk-admin";
import { bootstrapTenant } from "./fixtures/bootstrap";
import { FIXTURE_KEY } from "./fixtures/constants";
import { loadWorktreeEnv } from "./fixtures/env-loader";

// Defensive: playwright.auth.config.ts loads worktree-root .env, but
// globalSetup is the actual fail-fast point for missing creds and should
// re-load idempotently in case it's invoked outside the standard config.
loadWorktreeEnv();

const REQUIRED_ENV = [
  "NEXT_PUBLIC_API_URL",
  "CLERK_SECRET_KEY",
  "CLERK_WEBHOOK_SECRET",
] as const;

const FIXTURE_USERS: FixtureUserSpec[] = [
  {
    key: FIXTURE_KEY.regular,
    email: "regular-fixture@mailinator.com",
    password: "RegularFixture!2026-stable",
  },
  {
    key: FIXTURE_KEY.admin,
    email: "admin-fixture@mailinator.com",
    password: "AdminFixture!2026-stable",
  },
];

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

function failLoud(missing: string): never {
  throw new Error(
    `[e2e:auth] refusing to start: missing required env var ${missing}\n` +
      `Set in the workflow / shell before running:\n` +
      `  NEXT_PUBLIC_API_URL=https://api-dev.usezombie.com   # or other safe target\n` +
      `  CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')\n` +
      `  CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret')\n`,
  );
}

function writeCache(fixtures: MintedFixture[]): void {
  const cache: Record<string, Omit<MintedFixture, "key">> = {};
  for (const f of fixtures) {
    cache[f.key] = {
      email: f.email,
      password: f.password,
      clerkUserId: f.clerkUserId,
      sessionJwt: f.sessionJwt,
    };
  }
  fs.writeFileSync(JWT_CACHE_PATH, JSON.stringify(cache, null, 2), { mode: 0o600 });
}

export default async function globalSetup(): Promise<void> {
  for (const key of REQUIRED_ENV) {
    if (!process.env[key]) failLoud(key);
  }
  await clerkSetup();
  // Three-phase to keep JWT claims fresh:
  //   1. provisionUser: ensure each Clerk user exists (no JWT yet).
  //   2. bootstrapTenant: zombied creates tenant + writes tenant_id/role
  //      back to Clerk publicMetadata.
  //   3. attachJwt: mint session JWT — now the JWT snapshots the updated
  //      publicMetadata, so zombied API calls that require tenant context
  //      succeed.
  const provisioned = await Promise.all(FIXTURE_USERS.map(provisionUser));
  for (const user of provisioned) {
    await bootstrapTenant(user);
  }
  const fixtures: MintedFixture[] = [];
  for (const user of provisioned) {
    fixtures.push(await attachJwt(user));
  }
  writeCache(fixtures);
  console.log(
    `[e2e:auth] env present (api=${process.env.NEXT_PUBLIC_API_URL}); ` +
      `${fixtures.length} fixture users provisioned in Clerk + bootstrapped in zombied; ` +
      `JWTs cached to ${JWT_CACHE_PATH}`,
  );
}
