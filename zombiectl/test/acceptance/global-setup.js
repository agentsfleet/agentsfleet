/**
 * Pre-suite hook for the acceptance harness.
 *
 * Resolves `CLERK_SECRET_KEY` + the fixture email (op:// resolution happens
 * at the CI layer; this helper only reads the resolved values from env).
 * Optionally mints a session JWT for the `regular` fixture and writes it
 * to `test/acceptance/.fixture-jwt` (mode 0600) so per-spec spawns can
 * read it without re-minting.
 *
 * Specs that don't need a JWT (e.g. `help-and-errors.spec.js`) can skip
 * `ensureFixtureJwt` and only call `resolveAcceptanceEnv` for the API URL.
 */

import fs from "node:fs/promises";
import path from "node:path";
import url from "node:url";

import {
  ACCEPTANCE_TARGET_ENV,
  FIXTURE_JWT_FILE,
} from "./fixtures/constants.js";
import { attachJwt } from "./fixtures/clerk-admin.js";

const HERE = path.dirname(url.fileURLToPath(import.meta.url));
const ZOMBIECTL_ROOT = path.resolve(HERE, "..", "..");
const JWT_PATH = path.join(ZOMBIECTL_ROOT, FIXTURE_JWT_FILE);

export function resolveAcceptanceEnv() {
  const target = process.env[ACCEPTANCE_TARGET_ENV];
  if (!target) {
    throw new Error(`${ACCEPTANCE_TARGET_ENV} unset — acceptance suite requires an API URL`);
  }
  return { apiUrl: target };
}

export function resolveClerkSecret() {
  const secret = process.env.CLERK_SECRET_KEY;
  if (!secret) throw new Error("CLERK_SECRET_KEY missing — op:// resolution must run at the workflow layer");
  return secret;
}

export function resolveFixtureEmail(key) {
  const envName = key === "admin" ? "AUTH_E2E_ADMIN_EMAIL" : "AUTH_E2E_REGULAR_EMAIL";
  const value = process.env[envName];
  if (!value) {
    throw new Error(`${envName} unset — workflow must resolve op://VAULT/e2e-fixtures/${key}/email`);
  }
  if (/@mailinator\./i.test(value)) {
    throw new Error(`${envName} resolved to a mailinator domain — fixture-vault merge-gate violated`);
  }
  return value;
}

export async function ensureFixtureJwt() {
  const cached = await readCachedJwt();
  if (cached && !isExpired(cached)) return cached;
  const clerkSecret = resolveClerkSecret();
  const email = resolveFixtureEmail("regular");
  const minted = await attachJwt(clerkSecret, { email });
  const record = {
    sessionId: minted.sessionId,
    sessionJwt: minted.sessionJwt,
    clerkUserId: minted.clerkUserId,
    mintedAt: Date.now(),
  };
  await fs.mkdir(path.dirname(JWT_PATH), { recursive: true });
  await fs.writeFile(JWT_PATH, JSON.stringify(record), { mode: 0o600 });
  return record;
}

async function readCachedJwt() {
  try {
    const raw = await fs.readFile(JWT_PATH, "utf8");
    return JSON.parse(raw);
  } catch {
    return null;
  }
}

function isExpired(record) {
  const ageSec = (Date.now() - (record.mintedAt ?? 0)) / 1000;
  return ageSec >= 800;
}

export function fixtureJwtPath() {
  return JWT_PATH;
}
