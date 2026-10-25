import { timingSafeEqual } from "node:crypto";

export const CALLBACK_PATH = "/cli-auth/callback";
export const LOOPBACK_VERSION = "1";
export const LOOPBACK_FLOW = "loopback";
export const LOOPBACK_FLOW_PARAM = "flow";
export const LOOPBACK_HOST = "127.0.0.1";
export const MAX_CALLBACK_BODY_BYTES = 1_024;
export const MAX_CALLBACK_HEADER_BYTES = 4_096;
export const CALLBACK_READ_TIMEOUT_MS = 5_000;
export const CALLBACK_IDLE_TIMEOUT_SECONDS = 5;
export const MAX_LOGIN_WAIT_MS = 300_000;
export const CALLBACK_FORM_TYPE = "application/x-www-form-urlencoded";
export const CALLBACK_STATUS = { approved: "approved", denied: "denied" } as const;
export const CALLBACK_DECISION = { canceled: "canceled", expired: "expired" } as const;

export type CallbackSubmission =
  | { readonly status: typeof CALLBACK_STATUS.approved; readonly verificationCode: string }
  | { readonly status: typeof CALLBACK_STATUS.denied };
export type CallbackDecision = CallbackSubmission
  | { readonly status: typeof CALLBACK_DECISION.canceled | typeof CALLBACK_DECISION.expired };

const FINGERPRINT_DIGITS = 8;
const FINGERPRINT_GROUP = 4;
const FINGERPRINT_DOMAIN = "agentsfleet.cli.login.loopback.v1";
const HASH_SHA256 = "SHA-256";
const STATE_BYTES = 32;
const VERIFICATION_CODE_RE = /^\d{6}$/;
const CALLBACK_STATE_RE = /^[A-Za-z0-9_-]{43}$/;
const FIELD = { version: "v", session: "session_id", key: "cli_public_key", callback: "callback", state: "state", status: "status", code: "verification_code" } as const;
const CALLBACK_FIELDS = new Set<string>([FIELD.state, FIELD.session, FIELD.status, FIELD.code]);

export function trustedLoginUrl(dashboardUrl: string, loginUrl: string, sessionId: string): URL {
  const dashboard = new URL(dashboardUrl);
  const login = new URL(loginUrl);
  if (!/^https?:$/.test(dashboard.protocol) || dashboard.username || dashboard.password ||
      dashboard.search || dashboard.hash || dashboard.pathname !== "/" ||
      login.origin !== dashboard.origin || login.username || login.password || login.search || login.hash ||
      login.pathname !== `/cli-auth/${encodeURIComponent(sessionId)}`) {
    throw new Error("the login URL does not match the configured dashboard");
  }
  return login;
}

export async function loginComparisonCode(sessionId: string, publicKeyBase64Url: string): Promise<string> {
  const input = new TextEncoder().encode(`${FINGERPRINT_DOMAIN}\n${sessionId}\n${publicKeyBase64Url}`);
  const digest = new Uint8Array(await crypto.subtle.digest(HASH_SHA256, input));
  const code = Buffer.from(digest).toString("hex").slice(0, FINGERPRINT_DIGITS).toUpperCase();
  return `${code.slice(0, FINGERPRINT_GROUP)}-${code.slice(FINGERPRINT_GROUP)}`;
}

export function callbackState(): string {
  return Buffer.from(crypto.getRandomValues(new Uint8Array(STATE_BYTES))).toString("base64url");
}

export function browserHandoffUrl(login: URL, sessionId: string, publicKey: string, callback: string, state: string): string {
  const url = new URL(login);
  url.searchParams.set(LOOPBACK_FLOW_PARAM, LOOPBACK_FLOW);
  url.hash = new URLSearchParams({
    [FIELD.version]: LOOPBACK_VERSION, [FIELD.session]: sessionId, [FIELD.key]: publicKey,
    [FIELD.callback]: callback, [FIELD.state]: state,
  }).toString();
  return url.href;
}

export function parseCallbackSubmission(body: string, sessionId: string, state: string): CallbackSubmission | null {
  const fields = new URLSearchParams(body);
  for (const name of fields.keys()) {
    if (!CALLBACK_FIELDS.has(name) || fields.getAll(name).length !== 1) return null;
  }
  const suppliedState = fields.get(FIELD.state) ?? "";
  if (!CALLBACK_STATE_RE.test(suppliedState) || !timingSafeEqual(Buffer.from(suppliedState), Buffer.from(state)) ||
      fields.get(FIELD.session) !== sessionId) return null;
  const status = fields.get(FIELD.status);
  const code = fields.get(FIELD.code);
  if (status === CALLBACK_STATUS.denied && code === null) return { status };
  if (status === CALLBACK_STATUS.approved && code !== null && VERIFICATION_CODE_RE.test(code)) {
    return { status, verificationCode: code };
  }
  return null;
}
