import { afterAll } from "bun:test";

import { revokeMintedSessions } from "./fixtures/clerk-admin.ts";
import { REVOKE_WORST_CASE_MS } from "./fixtures/cli-credential-revoke.ts";
import { revokeHydratedCliCredentials } from "./fixtures/workspace-hydration.ts";
import { resolveAcceptanceEnv } from "./global-setup.ts";

/** What the Clerk session revokes may take after the credentials are done. */
const CLERK_REVOKE_BUDGET_MS = 20_000;
/**
 * The credential revokes' own worst case plus the Clerk budget. Explicit
 * because a by-hand `bun test --preload` would otherwise get bun's 5s hook
 * default and cut the loop mid-retry; the lane's `--timeout` is larger.
 */
const TEARDOWN_TIMEOUT_MS = REVOKE_WORST_CASE_MS + CLERK_REVOKE_BUDGET_MS;

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
