/**
 * signInAs(page, fixtureKey) — establish a Clerk session in the browser
 * for a fixture user without driving the hosted SignIn form.
 *
 * Implementation: the harness mints a one-time sign-in ticket through Clerk's
 * Backend API, then evaluates clerk-js in the page to call
 * `Clerk.signIn.create({strategy: 'ticket', ticket})` and
 * `Clerk.setActive({session, navigate})`. The no-op navigation callback keeps
 * Clerk from launching a route transition before the scenario chooses its
 * destination. The cookies that come out the other side
 * (`__session`, `__client_uat`, `__clerk_db_jwt`) are written by clerk-js
 * itself, so they carry the same shape clerkMiddleware expects from a
 * real interactive sign-in — including the `azp` claim on the session
 * JSON Web Token (JWT), which a Backend-API-minted default token omits.
 *
 * Why we cannot manually `addCookies` here: the previous implementation
 * minted the `__session` cookie via `POST /v1/sessions/{id}/tokens` and
 * stuffed `__clerk_db_jwt = "fixture-dev-browser"`. clerkMiddleware
 * accepted that on plain GETs but rejected it on the first Server-Action
 * round-trip (current `@clerk/nextjs` warns
 * `Session token from cookie is missing the azp claim`, and at the same
 * time clears `__client_uat` to `0`). The next protected navigation then
 * 302s to /sign-in. The ticket route avoids the entire mismatch
 * because clerk-js mints the cookies the way clerkMiddleware was built
 * to consume them.
 *
 * `setupClerkTestingToken({ page })` runs first to attach
 * `__clerk_testing_token` to every browser-side Frontend API (FAPI) call —
 * bypasses Clerk's bot challenge in development (Cloudflare Turnstile is now on by default for
 * the SignUp form) and keeps the testing posture stable across instance
 * config drift.
 *
 * Pre-req: globalSetup ran (`provisionUser` → `bootstrapTenant` → `attachJwt`)
 * and wrote the fixture-JWT cache.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { Page } from "@playwright/test";
import { clerk, setupClerkTestingToken } from "@clerk/testing/playwright";
import { createSignInTicket } from "./clerk-admin";
import type { FixtureKey } from "./constants";

export type { FixtureKey } from "./constants";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

interface FixtureCacheEntry {
  email: string;
  clerkUserId: string;
  sessionJwt: string;
}

interface JwtCache {
  [key: string]: FixtureCacheEntry;
}

function loadCache(): JwtCache {
  if (!fs.existsSync(JWT_CACHE_PATH)) {
    throw new Error(
      `Fixture JWT cache missing at ${JWT_CACHE_PATH}. globalSetup must run before signInAs.`,
    );
  }
  return JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as JwtCache;
}

function getFixtureEntry(cache: JwtCache, key: FixtureKey): FixtureCacheEntry {
  const entry = cache[key];
  if (!entry) {
    throw new Error(`No fixture entry for key '${key}'. Available: ${Object.keys(cache).join(", ")}`);
  }
  return entry;
}

export async function signInAs(page: Page, key: FixtureKey): Promise<void> {
  const cache = loadCache();
  const entry = getFixtureEntry(cache, key);
  await setupClerkTestingToken({ page });
  // clerk-js needs a Clerk-aware page mounted before it can mint a session.
  // /sign-in is the cheapest such page in the dashboard (no API fetches in
  // the Server Component), and it survives a redirect from any protected
  // route a future caller might land on first.
  await page.goto("/sign-in");
  await page.waitForFunction(() => Boolean(window.Clerk?.client));
  const hasActiveSession = await page.evaluate(() => Boolean(window.Clerk.session));
  if (hasActiveSession) {
    await clerk.signOut({ page });
    await page.goto("/sign-in");
    await page.waitForFunction(() => Boolean(window.Clerk?.client));
  }
  const ticket = await createSignInTicket(entry.clerkUserId);
  await page.evaluate(async (signInTicket) => {
    const client = window.Clerk.client;
    if (!client) throw new Error("Clerk client unavailable during fixture sign-in");
    const attempt = await client.signIn.create({
      strategy: "ticket",
      ticket: signInTicket,
    });
    if (attempt.status !== "complete" || !attempt.createdSessionId) {
      throw new Error(`fixture ticket sign-in did not complete (${attempt.status})`);
    }
    await window.Clerk.setActive({
      session: attempt.createdSessionId,
      navigate: async () => {},
    });
  }, ticket);
  await page.waitForFunction(() => Boolean(window.Clerk?.user));
  await page.goto("about:blank");
}

export function fixtureEmail(key: FixtureKey): string {
  return getFixtureEntry(loadCache(), key).email;
}

/**
 * Force clerk-js to refresh the `__session` cookie NOW. Session tokens live
 * ~60 seconds; a journey step that outlives that (GitHub import, install
 * stream, observe walks) leaves the next Server Action POST holding a stale
 * cookie — clerkMiddleware cannot handshake a POST, treats it as
 * unauthenticated, and 307s to /sign-in, silently losing the mutation.
 * Call this immediately before any mutation that follows a long wait.
 */
export async function refreshBrowserSession(page: Page): Promise<void> {
  await page.evaluate(async () => {
    await window.Clerk?.session?.getToken({ skipCache: true });
  });
}
