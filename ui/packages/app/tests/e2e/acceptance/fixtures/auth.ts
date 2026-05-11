/**
 * signInAs(page, fixtureKey) — mounts the cookies clerkMiddleware needs to
 * accept a fixture user as signed in, skipping Clerk's interactive sign-in
 * flow entirely.
 *
 * Three cookies are required for a Clerk DEV instance, not one
 * (per @clerk/backend authenticateRequest, chunk-ZFGNNAZZ.mjs:6075–6190):
 *
 *   1. `__session`      — the default session JWT (carries sid + sub + sts).
 *   2. `__client_uat`   — UNIX seconds, must be `<= jwt.iat`. Drives
 *                          `hasActiveClient`. Without it, middleware sees
 *                          session-without-client → redirects to /sign-in.
 *   3. `__clerk_db_jwt` — DEV-browser identifier. Read truthy-only by the
 *                          middleware (no signature verification), so any
 *                          non-empty string passes the
 *                          `!hasDevBrowserToken → DevBrowserMissing` gate.
 *
 * The Bearer JWT to zombied is a separate token — minted with the `api`
 * template, returned via clientFor() in api-client.ts. clerkMiddleware
 * never sees that token; zombied's OIDC verifier does.
 *
 * Pre-req: globalSetup ran (provisionUser → bootstrapTenant → attachJwt)
 * and wrote the fixture-JWT cache.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import type { Page } from "@playwright/test";
import type { FixtureKey } from "./constants";

export type { FixtureKey } from "./constants";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

interface JwtCache {
  [key: string]: {
    email: string;
    password: string;
    clerkUserId: string;
    sessionJwt: string;
    cookieJwt: string;
  };
}

function loadCache(): JwtCache {
  if (!fs.existsSync(JWT_CACHE_PATH)) {
    throw new Error(
      `Fixture JWT cache missing at ${JWT_CACHE_PATH}. globalSetup must run before signInAs.`,
    );
  }
  return JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as JwtCache;
}

function dashboardUrl(): URL {
  // Mirror playwright.acceptance.config.ts's BASE_URL so the cookie scope matches
  // the dev server's host.
  const port = process.env.E2E_PORT ?? "3101";
  const base = process.env.BASE_URL ?? `http://localhost:${port}`;
  return new URL(base);
}

function decodeJwtIat(jwt: string): number {
  const payload = jwt.split(".")[1];
  if (!payload) throw new Error("malformed JWT (no payload segment)");
  const json = JSON.parse(Buffer.from(payload, "base64url").toString("utf8")) as { iat?: number };
  if (typeof json.iat !== "number") throw new Error("JWT missing iat claim");
  return json.iat;
}

export async function signInAs(page: Page, key: FixtureKey): Promise<void> {
  const cache = loadCache();
  const entry = cache[key];
  if (!entry) {
    throw new Error(
      `No fixture entry for key '${key}'. Available: ${Object.keys(cache).join(", ")}`,
    );
  }
  const url = dashboardUrl();
  const iat = decodeJwtIat(entry.cookieJwt);
  // clientUat must be `<= jwt.iat` (chunk-ZFGNNAZZ.mjs:6173). Setting it one
  // second earlier than iat is unambiguously valid.
  const clientUat = String(iat - 1);
  const baseAttrs = {
    domain: url.hostname,
    path: "/",
    sameSite: "Lax" as const,
    secure: url.protocol === "https:",
  };
  await page.context().addCookies([
    { name: "__session", value: entry.cookieJwt, httpOnly: true, ...baseAttrs },
    { name: "__client_uat", value: clientUat, httpOnly: false, ...baseAttrs },
    { name: "__clerk_db_jwt", value: "fixture-dev-browser", httpOnly: false, ...baseAttrs },
  ]);
}

export function fixtureEmail(key: FixtureKey): string {
  const cache = loadCache();
  const entry = cache[key];
  if (!entry) throw new Error(`No fixture for key '${key}'`);
  return entry.email;
}
