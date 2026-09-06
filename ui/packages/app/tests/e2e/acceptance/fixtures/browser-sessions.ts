// Playwright runs these fixtures in Node worker processes. A directory inherited
// from globalSetup gives each run its own ledger; one file per session avoids
// concurrent workers overwriting a shared JSON cache.
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { revokeSession } from "./clerk-admin";

const SESSION_DIRECTORY_ENV = "AGENTSFLEET_E2E_SESSION_DIRECTORY";
const SESSION_ID = /^sess_[A-Za-z0-9]+$/;

export function initializeBrowserSessions(): void {
  process.env[SESSION_DIRECTORY_ENV] = fs.mkdtempSync(
    path.join(os.tmpdir(), "agentsfleet-e2e-sessions-"),
  );
}

export function recordBrowserSession(sessionId: string): void {
  const directory = process.env[SESSION_DIRECTORY_ENV];
  if (!directory) throw new Error("Browser session ledger missing; globalSetup must run first");
  if (!SESSION_ID.test(sessionId)) throw new Error("Invalid Clerk browser session id");
  fs.writeFileSync(path.join(directory, sessionId), "", { mode: 0o600 });
}

export async function revokeBrowserSessions(): Promise<void> {
  const directory = process.env[SESSION_DIRECTORY_ENV];
  if (!directory || !fs.existsSync(directory)) return;
  let revoked = 0;
  for (const sessionId of fs.readdirSync(directory)) {
    if (!SESSION_ID.test(sessionId)) continue;
    try {
      await revokeSession(sessionId);
      fs.unlinkSync(path.join(directory, sessionId));
      revoked += 1;
    } catch (error) {
      // Keep a failed record for an explicit retry and still clean up every
      // other session. Never touch sessions outside this run's ledger.
      console.error(`[e2e:auth] browser session revocation failed; retry ledger ${directory}:`, error);
    }
  }
  console.log(`[e2e:auth] revoked ${revoked} browser session(s) on teardown`);
  if (fs.readdirSync(directory).length === 0) fs.rmdirSync(directory);
}
