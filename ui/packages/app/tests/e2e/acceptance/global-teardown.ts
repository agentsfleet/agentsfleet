/**
 * Authenticated e2e harness — global teardown.
 *
 * Counterpart to global-setup.ts. Two jobs:
 *
 * 1. Session revocation. globalSetup mints a Clerk session per fixture user;
 *    without revoking them, every suite run leaves N more sessions in the
 *    Clerk instance. The cached `.fixture-jwts.json` records each session id.
 *
 * 2. Stale fixture-user sweep. The per-run signup specs delete their own users
 *    in afterEach, but a crashed run, an interrupted CI job, or a failed
 *    delete leaves them behind — and those accumulated users are exactly the
 *    "anonymous creds could get in" loose end. The sweep deletes any per-run
 *    fixture user older than STALE_AFTER_MS, matched by the STRICT pattern
 *    below (never by domain alone), so every suite run self-heals the
 *    previous crashed one. Clerk's user.deleted webhook then hard-purges the
 *    bootstrapped tenant daemon-side (state/account_teardown.zig).
 *
 * Runs automatically as Playwright's globalTeardown, and directly by hand:
 *
 *   cd ui/packages/app/tests/e2e/acceptance && bun global-teardown.ts
 *
 * Never throws: a teardown failure must not mask the suite's real result.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { deleteUser, listUsersByQuery, revokeSession } from "./fixtures/clerk-admin";
import { loadWorktreeEnv } from "./fixtures/env-loader";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

/** Old enough that no live suite run can still be using the user. */
const STALE_AFTER_MS = 60 * 60 * 1000;

/**
 * Exactly the addresses the per-run specs mint — prefix, 8-hex tag,
 * `+clerk_test`, and one of the two fixture domains (current + the retired
 * mailinator one, so legacy leftovers keep getting reaped). The persistent
 * regular/admin/operator fixtures deliberately do NOT match: they are
 * provisioned on purpose and re-used across runs. A real user can never
 * match this shape, which is the safety boundary of a sweep that deletes.
 */
export const PER_RUN_FIXTURE_RE =
  /^(signup-fixture|signup-webhook|signup-lifecycle|workspace-create)-[0-9a-f]{8}\+clerk_test@(e2e\.agentsfleet\.net|mailinator\.com)$/i;

const SWEEP_QUERIES = ["+clerk_test@e2e.agentsfleet.net", "+clerk_test@mailinator.com"];

interface CachedFixture {
  sessionId?: string;
}

async function revokeCachedSessions(): Promise<void> {
  if (!fs.existsSync(JWT_CACHE_PATH)) return;
  let cache: Record<string, CachedFixture>;
  try {
    cache = JSON.parse(fs.readFileSync(JWT_CACHE_PATH, "utf8")) as Record<string, CachedFixture>;
  } catch {
    return;
  }
  const sessionIds = Object.values(cache)
    .map((entry) => entry.sessionId)
    .filter((sid): sid is string => typeof sid === "string" && sid.length > 0);
  if (sessionIds.length === 0) return;
  await Promise.all(sessionIds.map(revokeSession));
  console.log(`[e2e:auth] revoked ${sessionIds.length} Clerk session(s) on teardown`);
}

async function sweepStaleFixtureUsers(): Promise<void> {
  if (!process.env.CLERK_SECRET_KEY) {
    console.log("[e2e:sweep] CLERK_SECRET_KEY unset — skipping stale-fixture sweep");
    return;
  }
  const now = Date.now();
  const seen = new Set<string>();
  let swept = 0;
  for (const query of SWEEP_QUERIES) {
    const users = await listUsersByQuery(query).catch((err: unknown) => {
      console.error(`[e2e:sweep] list failed for "${query}":`, err);
      return [];
    });
    for (const user of users) {
      if (seen.has(user.id)) continue;
      seen.add(user.id);
      if (!PER_RUN_FIXTURE_RE.test(user.email)) continue;
      if (now - user.createdAtMs < STALE_AFTER_MS) continue;
      try {
        await deleteUser(user.id);
        swept += 1;
        console.log(`[e2e:sweep] deleted stale fixture user ${user.email} (${user.id})`);
      } catch (err) {
        console.error(`[e2e:sweep] delete failed for ${user.email} (${user.id}):`, err);
      }
    }
  }
  console.log(`[e2e:sweep] done — ${swept} stale fixture user(s) removed`);
}

export default async function globalTeardown(): Promise<void> {
  try {
    await revokeCachedSessions();
  } catch (err) {
    console.error("[e2e:auth] session revocation failed:", err);
  }
  try {
    await sweepStaleFixtureUsers();
  } catch (err) {
    console.error("[e2e:sweep] sweep failed:", err);
  }
}

// Direct execution: `bun global-teardown.ts` (loads worktree .env for the key).
if (import.meta.main) {
  loadWorktreeEnv();
  await globalTeardown();
}
