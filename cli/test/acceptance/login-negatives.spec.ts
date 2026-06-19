/**
 * Login negative + alternate-auth acceptance scenarios for `agentsfleet`.
 *
 * Covers the failure and non-browser auth paths the happy-path login spec
 * (`lifecycle-after-login.spec.ts`) does not:
 *
 *   1. wrong code → re-prompt → correct code (pty + Chromium approve leg):
 *      "000000" is a valid 6-digit shape but a wrong HMAC, so the server
 *      returns 400 UZ-AUTH-011 (retryable). The CLI warns and re-prompts;
 *      the real code then completes the flow (exit 0, credentials.json).
 *   2. SIGINT at the code prompt (pty Ctrl-C) → exit 130, nothing persisted.
 *   3. `login --token <jwt>` direct path → persists, no browser.
 *   4. piped-stdin token (`login` with stdin carrying the JWT) → persists.
 *   5. `login --no-input` while already authed → aborts loudly (exit 130),
 *      existing credential untouched.
 *   6. `logout` after a token login → a subsequent read command is
 *      auth-required (non-zero).
 *   7. `auth status` before login → unauthenticated; after → shows identity.
 *
 * Live-only: the whole suite registers only when AGENTSFLEET_ACCEPTANCE_TARGET
 * is an https URL — the pty browser legs need the live dashboard + Clerk, and
 * the token paths still validate against the live /me probe. Without the gate
 * every test skips cleanly (matches the unit-test runner's local invariant).
 *
 * WS-E #C1 regression: assertNoSecretLeak fires after every spawn.
 */

import { describe, it, beforeAll, afterAll, beforeEach } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { composeEnv, runAgentctl } from "./fixtures/cli.js";
import type { RunResult } from "./fixtures/cli.js";
import { PtyProcess } from "./fixtures/pty.ts";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveDashboardUrl,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { completeCliAuthHandoff } from "./fixtures/browser.ts";
import {
  CREDENTIALS_FILENAME,
  assertPersistedCredential,
  credentialHasToken,
  credentialsExist,
  readCredentials,
} from "./fixtures/login-negatives-ops.ts";

const target = process.env.AGENTSFLEET_ACCEPTANCE_TARGET ?? "";
const isLive = target.startsWith("https://");

// Only the wrong-code retry drives the dashboard browser leg (clerk.signIn).
// Gated behind an explicit opt-in + the publishable key: clerk.signIn still
// times out on window.Clerk.loaded against the deployed dashboard pending a
// Clerk publishable-key/instance alignment (see lifecycle-after-login.spec).
// The SIGINT, --token, piped-stdin, logout, and auth-status paths need no
// browser leg and always run live.
const handshakeEnabled =
  process.env.AGENTSFLEET_ACCEPTANCE_LOGIN_HANDSHAKE === "1" && Boolean(process.env.CLERK_PUBLISHABLE_KEY);
const itHandshake = handshakeEnabled ? it : it.skip;

// printKeyValue renders the key space-aligned ("login_url   https://…"), not
// "login_url: …" — match an optional colon then whitespace before the URL.
const LOGIN_URL_RE = /login_url:?\s+(https?:\/\/\S+)/i;
const CODE_PROMPT_RE = /6-digit verification code/i;
const RETRY_WARN_RE = /didn't match|one more try/i;
const WRONG_CODE = "000000";
const HANDSHAKE_TIMEOUT_MS = 60_000;
const INTERRUPT_EXIT_CODE = 130;
const AUTH_REQUIRED_RE = /not authenticated|UZ-AUTH|run .*login/i;

// Argv verbs + flags hoisted per RULE UFS (any literal used >= 2x is a
// named const). The subcommand names + flag spellings are the wire contract
// asserted against src/program/cli-tree.ts.
const CMD_LOGIN = "login" as const;
const CMD_LOGOUT = "logout" as const;
const CMD_AUTH = "auth" as const;
const SUB_STATUS = "status" as const;
const CMD_WORKSPACE = "workspace" as const;
const SUB_LIST = "list" as const;
const FLAG_JSON = "--json" as const;
const FLAG_TOKEN = "--token" as const;
const FLAG_NO_OPEN = "--no-open" as const;
const FLAG_NO_INPUT = "--no-input" as const;
const AUTH_REQUIRED_CODE = "AUTH_REQUIRED" as const;
const SOURCE_FILE = "file" as const;

function parseLoginUrl(output: string): string {
  const match = output.match(LOGIN_URL_RE);
  if (!match || !match[1]) {
    throw new Error(`could not find login_url in CLI output: ${output.slice(0, 400)}`);
  }
  return match[1];
}

function rewriteHost(loginUrl: string, dashboardBase: string): string {
  // Preserve path + query (carries session_id); swap only host + protocol
  // so a localhost dashboard override still hits the right session page.
  const src = new URL(loginUrl);
  const dst = new URL(dashboardBase);
  src.protocol = dst.protocol;
  src.host = dst.host;
  return src.toString();
}

if (!isLive) {
  describe("login-negatives.spec.ts", () => {
    it.skip("requires AGENTSFLEET_ACCEPTANCE_TARGET to be an https URL", () => {});
  });
} else {
  describe("login-negatives — failure + alternate-auth paths", () => {
    let apiUrl: string = "";
    let dashboardUrl: string = "";
    let sessionJwt: string = "";
    let fixtureEmail: string = "";
    let stateDir: string = "";
    let baseEnv: Record<string, string> = {};
    let credentialsPath: string = "";

    async function spawn(
      args: ReadonlyArray<string>,
      extra?: { env?: Record<string, string>; stdin?: string },
    ): Promise<RunResult> {
      const env = extra?.env ? { ...baseEnv, ...extra.env } : baseEnv;
      const opts = extra?.stdin !== undefined ? { env, stdin: extra.stdin } : { env };
      const result = await runAgentctl(args, opts);
      assertNoSecretLeak(result, sessionJwt);
      return result;
    }

    async function freshStateDir(): Promise<void> {
      // Each token-path scenario starts from an empty state dir so an
      // earlier persisted credential never masks a missing write.
      await fs.rm(credentialsPath, { force: true });
    }

    beforeAll(async () => {
      apiUrl = resolveAcceptanceEnv().apiUrl;
      dashboardUrl = resolveDashboardUrl(apiUrl);
      const clerkSecret = resolveClerkSecret();
      fixtureEmail = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email: fixtureEmail });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-login-neg-"));
      credentialsPath = path.join(stateDir, CREDENTIALS_FILENAME);
      baseEnv = composeEnv({
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: "1",
        // AGENTSFLEET_TOKEN intentionally absent — the token-path scenarios
        // supply it explicitly (--token / piped stdin) and the rest prove
        // credentials.json is the load-bearing auth source.
      });
    });

    afterAll(async () => {
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // 7a — `auth status` with no credentials is unauthenticated. The
    // preAction auth-guard exempts only `login`, so `auth status` fails the
    // guard (AUTH_REQUIRED on stderr) before reaching authStatusEffect — the
    // observable "unauthenticated" contract for a credential-less invocation.
    describe("auth status before login", () => {
      beforeEach(freshStateDir);

      it("auth status --json → AUTH_REQUIRED, non-zero", async () => {
        const result = await spawn([CMD_AUTH, SUB_STATUS, FLAG_JSON]);
        assert.notEqual(result.code, 0, `auth status should fail unauth; stdout=${result.stdout}`);
        const parsed = JSON.parse(result.stderr.trim()) as { error?: { code?: string } };
        assert.equal(parsed.error?.code, AUTH_REQUIRED_CODE,
          `expected ${AUTH_REQUIRED_CODE} on stderr; got stderr=${result.stderr} stdout=${result.stdout}`);
      });
    });

    // 3 — direct `--token <jwt>` path: persists, no browser session.
    describe("login --token (direct path)", () => {
      beforeEach(freshStateDir);

      it("persists credentials.json without a device-flow session", async () => {
        const result = await spawn([CMD_LOGIN, FLAG_TOKEN, sessionJwt, FLAG_JSON]);
        assert.equal(result.code, 0, `login --token exited ${result.code}: ${result.stderr}`);
        const persisted = await assertPersistedCredential(credentialsPath);
        assert.equal(persisted, sessionJwt, "persisted token must equal the supplied --token value");
        const record = await readCredentials(credentialsPath);
        // Direct token has no device-flow session to label — `saveDirectToken`
        // writes session_id:null (on-disk key is snake_case session_id).
        assert.ok(!record?.session_id, `direct token must persist with no session_id; got ${record?.session_id}`);
        // No browser-session URL announced on the direct path.
        assert.ok(!LOGIN_URL_RE.test(result.stdout), `direct path must not announce a login_url: ${result.stdout}`);
      });
    });

    // 7b — `auth status` after a token login surfaces the identity.
    describe("auth status after login", () => {
      beforeEach(freshStateDir);

      it("auth status --json → authenticated:true, source file", async () => {
        const login = await spawn([CMD_LOGIN, FLAG_TOKEN, sessionJwt, FLAG_JSON]);
        assert.equal(login.code, 0, `precondition login exited ${login.code}: ${login.stderr}`);
        const result = await spawn([CMD_AUTH, SUB_STATUS, FLAG_JSON]);
        assert.equal(result.code, 0, `auth status exited ${result.code}: ${result.stderr}`);
        const parsed = JSON.parse(result.stdout.trim()) as { authenticated?: boolean; source?: string };
        assert.equal(parsed.authenticated, true, `expected authenticated:true; got ${result.stdout}`);
        assert.equal(parsed.source, SOURCE_FILE, `expected source:${SOURCE_FILE}; got ${result.stdout}`);
      });
    });

    // 4 — piped-stdin token: a non-TTY stdin carrying the JWT persists it.
    describe("login with piped-stdin token", () => {
      beforeEach(freshStateDir);

      it("persists credentials.json from the piped token", async () => {
        const result = await spawn([CMD_LOGIN, FLAG_JSON], { stdin: sessionJwt });
        assert.equal(result.code, 0, `piped login exited ${result.code}: ${result.stderr}`);
        const persisted = await assertPersistedCredential(credentialsPath);
        assert.equal(persisted, sessionJwt, "persisted token must equal the piped stdin value");
      });
    });

    // 5 — already-authed + `--no-input` aborts loudly; existing cred kept.
    describe("login --no-input while already authed", () => {
      beforeEach(freshStateDir);

      it("aborts non-zero and leaves the existing credential untouched", async () => {
        const seed = await spawn([CMD_LOGIN, FLAG_TOKEN, sessionJwt, FLAG_JSON]);
        assert.equal(seed.code, 0, `seed login exited ${seed.code}: ${seed.stderr}`);
        const before = await readCredentials(credentialsPath);
        assert.ok(before?.token, "precondition: a credential must be persisted before the abort test");

        // --no-input + no --force + existing credential → idempotencyCheck
        // refuses to overwrite and exits via InterruptedError (130). Pass an
        // empty stdin so the non-TTY pipe carries no token to resolve.
        const result = await spawn([CMD_LOGIN, FLAG_NO_INPUT], { stdin: "" });
        assert.notEqual(result.code, 0, `already-authed --no-input must abort; got 0: ${result.stdout}`);

        const after = await readCredentials(credentialsPath);
        assert.equal(after?.token, before?.token, "existing credential must survive the aborted re-login");
      });
    });

    // 6 — logout clears the credential; a follow-on read is auth-required.
    describe("logout then read command", () => {
      beforeEach(freshStateDir);

      it("workspace list --json fails auth-required after logout", async () => {
        const login = await spawn([CMD_LOGIN, FLAG_TOKEN, sessionJwt, FLAG_JSON]);
        assert.equal(login.code, 0, `precondition login exited ${login.code}: ${login.stderr}`);

        const logout = await spawn([CMD_LOGOUT, FLAG_JSON]);
        assert.equal(logout.code, 0, `logout exited ${logout.code}: ${logout.stderr}`);
        // `clearCredentials` overwrites the record with token:null rather than
        // unlinking the file — the post-logout contract is "no usable token",
        // not "file gone". Assert token-emptiness, not absence.
        assert.equal(await credentialHasToken(credentialsPath), false,
          "logout must clear the persisted token");

        // With AGENTSFLEET_TOKEN absent and credentials.json cleared, a
        // protected read must fail the auth guard rather than hit the API.
        const read = await spawn([CMD_WORKSPACE, SUB_LIST, FLAG_JSON]);
        assert.notEqual(read.code, 0, `post-logout read must be auth-required; got 0: ${read.stdout}`);
        assert.match(`${read.stderr}\n${read.stdout}`, AUTH_REQUIRED_RE,
          `expected an auth-required stem; got stderr=${read.stderr} stdout=${read.stdout}`);
      });
    });

    // 2 — SIGINT at the code prompt aborts with exit 130, nothing persisted.
    describe("SIGINT at the verification-code prompt", () => {
      beforeEach(freshStateDir);

      it("Ctrl-C → exit 130, no credentials.json", async () => {
        const cli = PtyProcess.spawnAgentctl([CMD_LOGIN, FLAG_NO_OPEN], { env: baseEnv });
        try {
          await cli.waitForLine((line) => CODE_PROMPT_RE.test(line), HANDSHAKE_TIMEOUT_MS);
          cli.interrupt();
          const exitCode = await cli.exited;
          assert.equal(exitCode, INTERRUPT_EXIT_CODE,
            `SIGINT must exit ${INTERRUPT_EXIT_CODE}; got ${exitCode}; output=${cli.output}`);
        } finally {
          cli.kill();
        }
        assert.equal(await credentialsExist(credentialsPath), false,
          "a SIGINT-aborted login must persist nothing");
        assertNoSecretLeak({ stdout: cli.output, stderr: "" }, sessionJwt);
      }, HANDSHAKE_TIMEOUT_MS + 10_000);
    });

    // 1 — wrong code (valid shape, bad HMAC) re-prompts; the real code
    // then completes the flow. Drives the pty + the Chromium approve leg.
    describe("wrong verification code then correct", () => {
      beforeEach(freshStateDir);

      itHandshake("000000 → re-prompt → real code → exit 0 + credentials.json", async () => {
        const cli = PtyProcess.spawnAgentctl([CMD_LOGIN, FLAG_NO_OPEN], { env: baseEnv });
        try {
          const announced = await cli.waitForLine((line) => LOGIN_URL_RE.test(line), HANDSHAKE_TIMEOUT_MS);
          const handoffUrl = rewriteHost(parseLoginUrl(announced), dashboardUrl);
          const realCode = await completeCliAuthHandoff({
            loginUrl: handoffUrl,
            email: fixtureEmail,
            timeoutMs: HANDSHAKE_TIMEOUT_MS,
          });
          assert.notEqual(realCode, WRONG_CODE, "fixture sanity: real code must differ from the wrong code");

          await cli.waitForLine((line) => CODE_PROMPT_RE.test(line), HANDSHAKE_TIMEOUT_MS);
          cli.writeLine(WRONG_CODE);

          // Server 400 UZ-AUTH-011 → VerificationFailedError → the CLI warns
          // and loops back to the prompt (one wrong-code strike spent).
          await cli.waitForLine((line) => RETRY_WARN_RE.test(line), HANDSHAKE_TIMEOUT_MS);
          cli.writeLine(realCode);

          const exitCode = await cli.exited;
          assert.equal(exitCode, 0, `retry login exited ${exitCode}; output=${cli.output}`);
        } finally {
          cli.kill();
        }
        await assertPersistedCredential(credentialsPath);
        assertNoSecretLeak({ stdout: cli.output, stderr: "" }, sessionJwt);
      }, HANDSHAKE_TIMEOUT_MS + 30_000);
    });
  });
}
