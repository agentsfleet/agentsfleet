export const CALLBACK_PATH = "/cli-auth/callback";
export const LOOPBACK_VERSION = "1";
export const LOOPBACK_FLOW = "loopback";
export const LOOPBACK_FLOW_PARAM = "flow";
export const LOOPBACK_MAX_AGE_MS = 5 * 60 * 1_000;
export const LOOPBACK_RECORD_BYTES = 2_048;
export const LOOPBACK_KIND = { manual: "manual", invalid: "invalid", automatic: "automatic" } as const;
export const LOOPBACK_FAILURE = "This local sign-in could not be restored. Restart login in your terminal, or use --no-open for another device.";

const FINGERPRINT_DOMAIN = "agentsfleet.cli.login.loopback.v1";
const HASH_SHA256 = "SHA-256";
const FINGERPRINT_DIGITS = 8;
const FINGERPRINT_GROUP = 4;
const STATE_BYTES = 32;
const SPKI_BYTES = 91;
const SPKI_PREFIX = "3059301306072a8648ce3d020106082a8648ce3d03010703420004";
const HEX_RADIX = 16;
const MAX_PORT = 65_535;
const FIELD = { version: "v", session: "session_id", key: "cli_public_key", callback: "callback", state: "state" } as const;
const FIELD_NAMES = new Set<string>(Object.values(FIELD));
const SESSION_PATTERN = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const CALLBACK_PATTERN = new RegExp(`^http://127\\.0\\.0\\.1:([1-9][0-9]{0,4})${CALLBACK_PATH}$`);

export interface LoopbackHandoff {
  readonly sessionId: string;
  readonly publicKey: string;
  readonly callback: string;
  readonly state: string;
}

export type BrowserHandoff =
  | { kind: typeof LOOPBACK_KIND.manual }
  | { kind: typeof LOOPBACK_KIND.invalid; message: string }
  | { kind: typeof LOOPBACK_KIND.automatic; handoff: LoopbackHandoff };

function canonicalBytes(value: string, size: number): string | null {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;
  try {
    const decoded = atob(value.replaceAll("-", "+").replaceAll("_", "/"));
    const encoded = btoa(decoded).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
    return decoded.length === size && encoded === value ? decoded : null;
  } catch {
    return null;
  }
}

function validPublicKey(value: string): boolean {
  const bytes = canonicalBytes(value, SPKI_BYTES);
  if (bytes === null) return false;
  const prefix = Array.from(bytes.slice(0, SPKI_PREFIX.length / 2), (byte) =>
    byte.charCodeAt(0).toString(HEX_RADIX).padStart(2, "0")).join("");
  return prefix === SPKI_PREFIX;
}

export function parseLoopbackFragment(hash: string): LoopbackHandoff | null {
  if (hash.length > LOOPBACK_RECORD_BYTES || /%(?![0-9a-f]{2})/i.test(hash)) return null;
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  if (params.get(FIELD.version) !== LOOPBACK_VERSION) return null;
  const sessionId = params.get(FIELD.session);
  const publicKey = params.get(FIELD.key);
  const callback = params.get(FIELD.callback);
  const state = params.get(FIELD.state);
  if (sessionId === null || publicKey === null || callback === null || state === null) return null;
  // Presence of every field plus the exact count excludes duplicates and unknown fields.
  if (params.size !== FIELD_NAMES.size) return null;
  const port = CALLBACK_PATTERN.exec(callback)?.[1];
  if (!SESSION_PATTERN.test(sessionId) || !validPublicKey(publicKey)) return null;
  if (!port || Number(port) > MAX_PORT || canonicalBytes(state, STATE_BYTES) === null) return null;
  return { sessionId, publicKey, callback, state };
}

export function hasLoopbackFragment(hash: string): boolean {
  const params = new URLSearchParams(hash.replace(/^#/, ""));
  return params.has(FIELD.session) || params.has(FIELD.key) ||
    (params.has(FIELD.version) && params.has(FIELD.callback));
}

export async function matchingCode(handoff: LoopbackHandoff): Promise<string> {
  const transcript = `${FINGERPRINT_DOMAIN}\n${handoff.sessionId}\n${handoff.publicKey}`;
  const digest = await crypto.subtle.digest(HASH_SHA256, new TextEncoder().encode(transcript));
  const code = Array.from(new Uint8Array(digest), (byte) => byte.toString(HEX_RADIX).padStart(2, "0"))
    .join("").slice(0, FINGERPRINT_DIGITS).toUpperCase();
  return `${code.slice(0, FINGERPRINT_GROUP)}-${code.slice(FINGERPRINT_GROUP)}`;
}
