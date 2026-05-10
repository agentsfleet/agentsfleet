/**
 * Authenticated e2e harness — global setup.
 *
 * Runs once per suite before any auth spec. Responsibility (this commit):
 *   Fail fast if any required env var is missing, with a copy-paste recipe
 *   in the error body. Safety re: which environment we point at lives in the
 *   workflow that sets these vars — code does not second-guess it.
 *
 * Future commits add: fixture-user JWT mint via Clerk admin API, Svix-signed
 * bootstrap POST to /v1/webhooks/clerk so each fixture user has a tenant +
 * default workspace + $5 starter credit before any spec runs.
 */

const REQUIRED_ENV = [
  "NEXT_PUBLIC_API_URL",
  "CLERK_SECRET_KEY",
  "CLERK_WEBHOOK_SECRET",
] as const;

function failLoud(missing: string): never {
  throw new Error(
    `[e2e:auth] refusing to start: missing required env var ${missing}\n` +
      `Set in the workflow / shell before running:\n` +
      `  NEXT_PUBLIC_API_URL=https://api-dev.usezombie.com   # or other safe target\n` +
      `  CLERK_SECRET_KEY=$(op read 'op://ZMB_CD_DEV/clerk-dev/secret-key')\n` +
      `  CLERK_WEBHOOK_SECRET=$(op read 'op://ZMB_CD_DEV/clerk-dev/webhook-secret')\n`,
  );
}

export default async function globalSetup(): Promise<void> {
  for (const key of REQUIRED_ENV) {
    if (!process.env[key]) failLoud(key);
  }
  console.log(
    `[e2e:auth] env present (api=${process.env.NEXT_PUBLIC_API_URL}); fixture warm deferred to WS-A.2`,
  );
}
