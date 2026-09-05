import { afterAll } from "bun:test";

import { revokeMintedSessions } from "./fixtures/clerk-admin.ts";
import { revokeHydratedCliCredentials } from "./fixtures/workspace-hydration.ts";
import { resolveAcceptanceEnv } from "./global-setup.ts";

/**
 * Five revoke attempts of up to 10s each, the backoff between them, then the
 * Clerk session revokes: about 60s at worst. The lane passes a larger
 * `--timeout`, but a by-hand `bun test --preload` would otherwise get bun's
 * 5s hook default and cut the loop mid-retry.
 */
const TEARDOWN_TIMEOUT_MS = 90_000;

afterAll(async () => {
  const failures: unknown[] = [];
  await revokeHydratedCliCredentials(resolveAcceptanceEnv().apiUrl)
    .catch((error: unknown) => { failures.push(error); });
  // The sessions are revoked whether or not the credentials were: a failure
  // above must not leave the Clerk sessions this file minted live as well.
  await revokeMintedSessions()
    .catch((error: unknown) => { failures.push(error); });
  if (failures.length === 1) throw failures[0];
  if (failures.length > 1) {
    throw new AggregateError(failures, "credential and session teardown both failed");
  }
}, { timeout: TEARDOWN_TIMEOUT_MS });
