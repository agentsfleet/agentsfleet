/**
 * Slice-owned helpers for `login-negatives.spec.ts`.
 *
 * The shared fixtures (cli.js, pty.ts, browser.ts, …) cover spawning and
 * the browser approve leg; these helpers cover the credentials.json
 * assertions the negative-path spec needs and are NOT shared with other
 * slices — they live here so the spec stays under the file-length cap
 * without editing any shared fixture.
 *
 * On-disk schema note: the credentials.json record uses snake_case keys
 * (`token` / `saved_at` / `session_id` / `api_url`) — see
 * `src/lib/state.ts` + `src/services/credentials.ts`. The record type below
 * mirrors that wire shape exactly; using a camelCase key here would always
 * read `undefined` and make assertions pass for the wrong reason.
 */

import fs from "node:fs/promises";

// `credentials.json` is the on-disk auth record `Credentials.saveAccessToken`
// writes; the device-flow + direct-token + piped-stdin paths all funnel
// through it. Mirrors the filename `lifecycle-after-login.spec.ts` reads.
export const CREDENTIALS_FILENAME = "credentials.json";

// A persisted credential is mode 0600 (WS-E #C3). The pty handshake spec
// asserts this; the negative-path spec reuses the constant to prove the
// direct-token / piped-stdin writes carry the same posture.
export const CREDENTIALS_MODE = 0o600;

// A persisted JWT is a 3-segment string. The negative-path spec asserts
// the recovered token shape after each non-interactive login path.
export const JWT_SEGMENTS = 3;

// `logout` clears credentials by overwriting the record with a null token
// (it does NOT unlink the file — see `clearCredentials` in src/lib/state.ts).
// The post-logout assertion therefore checks token-emptiness, not absence.
export interface CredentialsRecord {
  readonly token: string | null;
  readonly saved_at?: number | null;
  readonly session_id?: string | null;
  readonly api_url?: string | null;
}

// Returns the parsed credentials.json, or null when the file is absent.
// The SIGINT-abort scenario asserts absence (nothing persisted on Ctrl-C);
// the direct-token / piped scenarios assert presence with a non-null token.
export async function readCredentials(
  credentialsPath: string,
): Promise<CredentialsRecord | null> {
  try {
    const raw = await fs.readFile(credentialsPath, "utf8");
    return JSON.parse(raw) as CredentialsRecord;
  } catch {
    return null;
  }
}

// True iff credentials.json exists on disk. Separate from readCredentials
// so the SIGINT scenario can assert non-existence without parsing.
export async function credentialsExist(credentialsPath: string): Promise<boolean> {
  try {
    await fs.stat(credentialsPath);
    return true;
  } catch {
    return false;
  }
}

// True iff credentials.json holds a non-empty token. `logout` overwrites the
// file with `{ token: null }` rather than deleting it, so the post-logout
// contract is "no usable token persisted", not "file gone".
export async function credentialHasToken(credentialsPath: string): Promise<boolean> {
  const record = await readCredentials(credentialsPath);
  return typeof record?.token === "string" && record.token.length > 0;
}

// Asserts the on-disk credential carries a 3-segment JWT and 0600 mode.
// Throws (not returns) on any violation so callers can `await` it directly
// inside a test body. Returns the parsed token for follow-on assertions.
export async function assertPersistedCredential(
  credentialsPath: string,
): Promise<string> {
  const stat = await fs.stat(credentialsPath);
  const mode = stat.mode & 0o777;
  if (mode !== CREDENTIALS_MODE) {
    throw new Error(
      `credentials.json mode is ${mode.toString(8)} — expected 600 (WS-E #C3)`,
    );
  }
  const record = await readCredentials(credentialsPath);
  if (!record || typeof record.token !== "string") {
    throw new Error(`credentials.json missing a string token: ${JSON.stringify(record)}`);
  }
  if (record.token.split(".").length !== JWT_SEGMENTS) {
    throw new Error(`persisted token is not a 3-segment JWT: ${record.token}`);
  }
  return record.token;
}
