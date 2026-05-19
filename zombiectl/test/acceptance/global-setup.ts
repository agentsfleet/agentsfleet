/**
 * Pre-suite hook for the acceptance harness.
 *
 * Resolves `CLERK_SECRET_KEY` + the fixture email (op:// resolution happens
 * at the CI layer; this helper only reads the resolved values from env).
 * Optionally mints a session JWT for the `regular` fixture and writes it
 * to `test/acceptance/.fixture-jwt` (mode 0600) so per-spec spawns can
 * read it without re-minting.
 *
 * Specs that don't need a JWT (e.g. `help-and-errors.spec.ts`) can skip
 * `ensureFixtureJwt` and only call `resolveAcceptanceEnv` for the API URL.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import {
  ACCEPTANCE_DASHBOARD_URL_ENV,
  ACCEPTANCE_TARGET_ENV,
  API_URL_DEV,
  API_URL_PROD,
  DASHBOARD_URL_DEV,
  DASHBOARD_URL_PROD,
  FIXTURE_JWT_FILE,
} from "./fixtures/constants.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const ZOMBIECTL_ROOT = path.resolve(HERE, "..", "..");
const JWT_PATH = path.join(ZOMBIECTL_ROOT, FIXTURE_JWT_FILE);

const JWT_TTL_SECONDS = 800;

export interface AcceptanceEnv {
  readonly apiUrl: string;
}

export interface FixtureJwtRecord {
  readonly sessionId: string;
  readonly sessionJwt: string;
  readonly clerkUserId: string;
  readonly mintedAt: number;
}

export type FixtureKey = "admin" | "regular";

export function resolveAcceptanceEnv(): AcceptanceEnv {
  const target = process.env[ACCEPTANCE_TARGET_ENV];
  if (!target) {
    throw new Error(`${ACCEPTANCE_TARGET_ENV} unset — acceptance suite requires an API URL`);
  }
  return { apiUrl: target };
}

/**
 * Derive the dashboard URL from the acceptance API URL. The dashboard
 * environment always pairs with the API environment, so the routing
 * is deterministic — no separate skip gate needed.
 *
 * Explicit `ZOMBIE_ACCEPTANCE_DASHBOARD_URL` override wins (use this
 * for `localhost:3000` against a locally-running dashboard).
 */
export function resolveDashboardUrl(apiUrl: string): string {
  const override = process.env[ACCEPTANCE_DASHBOARD_URL_ENV]?.trim();
  if (override) return override;
  if (apiUrl.startsWith(API_URL_DEV)) return DASHBOARD_URL_DEV;
  if (apiUrl.startsWith(API_URL_PROD)) return DASHBOARD_URL_PROD;
  throw new Error(
    `cannot derive dashboard URL for API ${apiUrl} — set ${ACCEPTANCE_DASHBOARD_URL_ENV} explicitly`,
  );
}

export function resolveClerkSecret(): string {
  const secret = process.env.CLERK_SECRET_KEY;
  if (!secret) throw new Error("CLERK_SECRET_KEY missing — op:// resolution must run at the workflow layer");
  return secret;
}

export function resolveFixtureEmail(key: FixtureKey): string {
  const envName = key === "admin" ? "AUTH_E2E_ADMIN_EMAIL" : "AUTH_E2E_REGULAR_EMAIL";
  const value = process.env[envName];
  if (!value) {
    throw new Error(`${envName} unset — workflow must resolve op://VAULT/e2e-fixtures-email/${key}`);
  }
  if (/@mailinator\./i.test(value)) {
    throw new Error(`${envName} resolved to a mailinator domain — fixture-vault merge-gate violated`);
  }
  return value;
}

export async function ensureFixtureJwt(): Promise<FixtureJwtRecord> {
  const cached = await readCachedJwt();
  if (cached && !isExpired(cached)) return cached;
  const clerkSecret = resolveClerkSecret();
  const email = resolveFixtureEmail("regular");
  const minted = await attachJwt(clerkSecret, { email });
  const record: FixtureJwtRecord = {
    sessionId: minted.sessionId,
    sessionJwt: minted.sessionJwt,
    clerkUserId: minted.clerkUserId,
    mintedAt: Date.now(),
  };
  await fs.mkdir(path.dirname(JWT_PATH), { recursive: true });
  await fs.writeFile(JWT_PATH, JSON.stringify(record), { mode: 0o600 });
  return record;
}

async function readCachedJwt(): Promise<FixtureJwtRecord | null> {
  try {
    const raw = await fs.readFile(JWT_PATH, "utf8");
    return JSON.parse(raw) as FixtureJwtRecord;
  } catch {
    return null;
  }
}

function isExpired(record: FixtureJwtRecord): boolean {
  const ageSec = (Date.now() - (record.mintedAt ?? 0)) / 1000;
  return ageSec >= JWT_TTL_SECONDS;
}

export function fixtureJwtPath(): string {
  return JWT_PATH;
}
