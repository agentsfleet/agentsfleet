/**
 * Authenticated e2e harness — global teardown.
 *
 * Counterpart to global-setup.ts. globalSetup mints a Clerk session per
 * fixture user (POST /v1/sessions); without revoking them, every suite
 * run leaves N more sessions sitting in the Clerk DEV instance. The
 * cached `.fixture-jwts.json` records each session id; this teardown
 * walks the cache and revokes them.
 *
 * Tolerates "already revoked" / "session not found" responses
 * (revokeSession swallows 4xx) so a partial setup cannot mask the
 * primary test failure.
 */
import * as fs from "node:fs";
import * as path from "node:path";
import { revokeSession } from "./fixtures/clerk-admin";

const JWT_CACHE_PATH = path.join(process.cwd(), ".fixture-jwts.json");

interface CachedFixture {
  sessionId?: string;
}

export default async function globalTeardown(): Promise<void> {
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
