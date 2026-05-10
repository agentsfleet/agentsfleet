/**
 * Authenticated e2e harness — global setup.
 *
 * Runs once per suite before any auth spec. Responsibilities (this commit):
 *   1. Refuse to run unless NEXT_PUBLIC_API_URL exact-matches the api-dev allow-list.
 *      Safety belt — keeps fixture rows out of staging/prod.
 *   2. Refuse to run unless CLERK_SECRET_KEY and CLERK_WEBHOOK_SECRET are present.
 *
 * Future commits add: fixture-user JWT mint via Clerk admin API, Svix-signed
 * bootstrap POST to /v1/webhooks/clerk so each fixture user has a tenant +
 * default workspace + $5 starter credit before any spec runs.
 */

const API_DEV_ALLOWLIST = "https://api-dev.usezombie.com";

const REQUIRED_ENV = [
  "NEXT_PUBLIC_API_URL",
  "CLERK_SECRET_KEY",
  "CLERK_WEBHOOK_SECRET",
] as const;

function failLoud(reason: string): never {
  throw new Error(
    `[e2e:auth] refusing to start: ${reason}\n` +
      `Required env vars (resolve via op read):\n` +
      `  NEXT_PUBLIC_API_URL=${API_DEV_ALLOWLIST}\n` +
      `  CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')\n` +
      `  CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret')\n`,
  );
}

export default async function globalSetup(): Promise<void> {
  for (const key of REQUIRED_ENV) {
    if (!process.env[key]) {
      failLoud(`missing required env var ${key}`);
    }
  }

  const apiUrl = process.env.NEXT_PUBLIC_API_URL;
  if (apiUrl !== API_DEV_ALLOWLIST) {
    failLoud(
      `NEXT_PUBLIC_API_URL='${apiUrl}' does not match the api-dev allow-list ` +
        `('${API_DEV_ALLOWLIST}'). Authenticated e2e suite is api-dev only.`,
    );
  }

  console.log(`[e2e:auth] env-guard passed; api-dev confirmed; fixture warm deferred to WS-A.2`);
}
