/**
 * signInAs(page, fixtureKey) — signs the page in as a fixture user via
 * Clerk's password strategy.
 *
 * Earlier iterations mounted the `__session` cookie directly. That works for
 * sending API calls to zombied (cookie carries the JWT) but not for
 * navigating the dashboard — Clerk's client SDK expects multiple cookies
 * (`__session`, `__client_uat`, etc.) and treats lone `__session` as
 * incomplete. @clerk/testing's `clerk.signIn` uses Clerk's real sign-in API
 * and produces a fully-cookied session that survives navigation.
 *
 * Pre-req: globalSetup must have called clerkSetup() and the fixture user
 * must exist in Clerk DEV (provisionUser handles both).
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { Page } from "@playwright/test";
import { clerk } from "@clerk/testing/playwright";
import type { MintedFixture } from "./clerk-admin";

export type FixtureKey = MintedFixture["key"];

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

interface JwtCache {
  [key: string]: { email: string; password: string; clerkUserId: string; sessionJwt: string };
}

function loadCache(): JwtCache {
  if (!fs.existsSync(JWT_CACHE_PATH)) {
    throw new Error(
      `Fixture JWT cache missing at ${JWT_CACHE_PATH}. globalSetup must run before signInAs.`,
    );
  }
  return JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as JwtCache;
}

export async function signInAs(page: Page, key: FixtureKey): Promise<void> {
  const cache = loadCache();
  const entry = cache[key];
  if (!entry) {
    throw new Error(`No fixture entry for key '${key}'. Available: ${Object.keys(cache).join(", ")}`);
  }
  await page.goto("/sign-in");
  await clerk.signIn({
    page,
    signInParams: {
      strategy: "password",
      identifier: entry.email,
      password: entry.password,
    },
  });
}

export function fixtureEmail(key: FixtureKey): string {
  const cache = loadCache();
  const entry = cache[key];
  if (!entry) throw new Error(`No fixture for key '${key}'`);
  return entry.email;
}
