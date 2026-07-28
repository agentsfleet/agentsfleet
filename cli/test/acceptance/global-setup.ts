/**
 * Pre-suite hook for the acceptance harness.
 *
 * Resolves `CLERK_SECRET_KEY` + the fixture email (op:// resolution happens
 * at the CI layer; this helper only reads the resolved values from env).
 * The live-lane entrypoint validates the browser/auth inputs and establishes
 * both suite-owned Clerk identities before any live spec starts. Deterministic
 * specs never invoke this file as an entrypoint and need no browser.
 */

import {
  ACCEPTANCE_DASHBOARD_URL_ENV,
  ACCEPTANCE_TARGET_ENV,
  API_URL_DEV,
  API_URL_PROD,
  DASHBOARD_URL_DEV,
  DASHBOARD_URL_PROD,
} from "./fixtures/constants.ts";
import { ensureFixtureTenantReady } from "./fixtures/clerk-admin.ts";

export interface AcceptanceEnv {
  readonly apiUrl: string;
}

export type FixtureKey = "admin" | "regular";

const LOGIN_HANDSHAKE_ENV = "AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE";
const CLERK_SECRET_ENV = "CLERK_SECRET_KEY";
const CLERK_PUBLISHABLE_KEY_ENV = "CLERK_PUBLISHABLE_KEY";
const CLERK_WEBHOOK_SECRET_ENV = "CLERK_WEBHOOK_SECRET";
const REGULAR_EMAIL_ENV = "AUTH_E2E_REGULAR_EMAIL";
const ADMIN_EMAIL_ENV = "AUTH_E2E_ADMIN_EMAIL";
const HANDSHAKE_ENABLED_VALUE = "1";
const HTTPS_PREFIX = "https://";
const FIXTURE_ROLE = {
  regular: "regular",
  admin: "admin",
} as const;

export interface LiveAcceptancePreflight {
  readonly apiUrl: string;
  readonly dashboardUrl: string;
  readonly clerkSecret: string;
  readonly regularEmail: string;
  readonly adminEmail: string;
}

export function resolveAcceptanceEnv(env: NodeJS.ProcessEnv = process.env): AcceptanceEnv {
  const target = env[ACCEPTANCE_TARGET_ENV];
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
 * Explicit `AGENTSFLEET_ACCEPTANCE_DASHBOARD_URL` override wins (use this
 * for `localhost:3000` against a locally-running dashboard).
 */
export function resolveDashboardUrl(
  apiUrl: string,
  env: NodeJS.ProcessEnv = process.env,
): string {
  const override = env[ACCEPTANCE_DASHBOARD_URL_ENV]?.trim();
  if (override) return override;
  if (apiUrl.startsWith(API_URL_DEV)) return DASHBOARD_URL_DEV;
  if (apiUrl.startsWith(API_URL_PROD)) return DASHBOARD_URL_PROD;
  throw new Error(
    `cannot derive dashboard URL for API ${apiUrl} — set ${ACCEPTANCE_DASHBOARD_URL_ENV} explicitly`,
  );
}

export function resolveClerkSecret(env: NodeJS.ProcessEnv = process.env): string {
  const secret = env[CLERK_SECRET_ENV];
  if (!secret) throw new Error(`${CLERK_SECRET_ENV} missing — op:// resolution must run at the workflow layer`);
  return secret;
}

export function resolveFixtureEmail(
  key: FixtureKey,
  env: NodeJS.ProcessEnv = process.env,
): string {
  const envName = key === FIXTURE_ROLE.admin ? ADMIN_EMAIL_ENV : REGULAR_EMAIL_ENV;
  const value = env[envName];
  if (!value) {
    throw new Error(`${envName} unset — workflow must resolve op://VAULT/e2e-fixtures-email/${key}`);
  }
  if (/@mailinator\./i.test(value)) {
    throw new Error(`${envName} resolved to a mailinator domain — fixture-vault merge-gate violated`);
  }
  return value;
}

export function resolveLiveAcceptancePreflight(
  env: NodeJS.ProcessEnv = process.env,
): LiveAcceptancePreflight {
  const { apiUrl } = resolveAcceptanceEnv(env);
  if (!apiUrl.startsWith(HTTPS_PREFIX)) {
    throw new Error(`${ACCEPTANCE_TARGET_ENV} must be an https URL for the live lane`);
  }
  if (env[LOGIN_HANDSHAKE_ENV] !== HANDSHAKE_ENABLED_VALUE) {
    throw new Error(`${LOGIN_HANDSHAKE_ENV} must equal ${HANDSHAKE_ENABLED_VALUE} for the live lane`);
  }
  requireEnv(env, CLERK_PUBLISHABLE_KEY_ENV);
  requireEnv(env, CLERK_WEBHOOK_SECRET_ENV);
  return {
    apiUrl,
    dashboardUrl: resolveDashboardUrl(apiUrl, env),
    clerkSecret: resolveClerkSecret(env),
    regularEmail: resolveFixtureEmail(FIXTURE_ROLE.regular, env),
    adminEmail: resolveFixtureEmail(FIXTURE_ROLE.admin, env),
  };
}

function requireEnv(env: NodeJS.ProcessEnv, name: string): string {
  const value = env[name]?.trim();
  if (!value) throw new Error(`${name} missing — live acceptance preflight failed`);
  return value;
}

async function verifyChromiumInstalled(): Promise<void> {
  const { chromium } = await import("playwright");
  const executablePath = chromium.executablePath();
  if (!(await Bun.file(executablePath).exists())) {
    throw new Error(`Playwright Chromium missing at ${executablePath}`);
  }
}

export async function runLiveAcceptancePreflight(
  env: NodeJS.ProcessEnv = process.env,
): Promise<void> {
  const config = resolveLiveAcceptancePreflight(env);
  await verifyChromiumInstalled();
  await ensureFixtureTenantReady(config.clerkSecret, {
    email: config.regularEmail,
    role: FIXTURE_ROLE.regular,
  });
  await ensureFixtureTenantReady(config.clerkSecret, {
    email: config.adminEmail,
    role: FIXTURE_ROLE.admin,
  });
}

if (import.meta.main) {
  try {
    await runLiveAcceptancePreflight();
    process.stdout.write("CLI live acceptance preflight passed\n");
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : String(error);
    process.stderr.write(`CLI live acceptance preflight failed: ${message}\n`);
    process.exitCode = 1;
  }
}
