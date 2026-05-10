/**
 * signInAs(page, fixtureKey) — mounts a Clerk session JWT for a fixture user.
 *
 * Reads the JWT cache written by global-setup.ts and sets Clerk's `__session`
 * cookie on the Playwright browser context. Subsequent navigations are
 * authenticated as the named fixture user; no UI sign-in clicks.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { Page } from "@playwright/test";
import type { MintedFixture } from "./clerk-admin";

export type FixtureKey = MintedFixture["key"];

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

interface JwtCache {
  [key: string]: { email: string; clerkUserId: string; sessionJwt: string };
}

function loadCache(): JwtCache {
  if (!fs.existsSync(JWT_CACHE_PATH)) {
    throw new Error(
      `Fixture JWT cache missing at ${JWT_CACHE_PATH}. ` +
        `globalSetup must run before signInAs.`,
    );
  }
  return JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as JwtCache;
}

export async function signInAs(page: Page, key: FixtureKey): Promise<void> {
  const cache = loadCache();
  const entry = cache[key];
  if (!entry) {
    throw new Error(`No fixture JWT for key '${key}'. Available: ${Object.keys(cache).join(", ")}`);
  }

  const target = new URL(page.url() === "about:blank" ? "http://localhost" : page.url());
  const hostname = process.env.BASE_URL ? new URL(process.env.BASE_URL).hostname : target.hostname;

  await page.context().addCookies([
    {
      name: "__session",
      value: entry.sessionJwt,
      domain: hostname,
      path: "/",
      httpOnly: false,
      secure: hostname !== "localhost",
      sameSite: "Lax",
    },
  ]);
}

export function fixtureEmail(key: FixtureKey): string {
  const cache = loadCache();
  const entry = cache[key];
  if (!entry) throw new Error(`No fixture for key '${key}'`);
  return entry.email;
}
