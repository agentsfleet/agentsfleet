import { describe, test, expect } from "bun:test";

import { runCli } from "../src/cli.ts";
import { saveCredentials } from "../src/lib/state.ts";
import { bufferStream, withFreshStateDir } from "./helpers-cli-state.ts";

interface RecordedCall { url: string; method: string }
interface FetchRecorder {
  calls: RecordedCall[];
  fetchImpl: (url: string, options?: { method?: string }) => Promise<ResponseLike>;
}

// Structural Response — runCli's apiRequest only reads ok/status/text.
interface ResponseLike {
  ok: boolean;
  status: number;
  text: () => Promise<string>;
}

// fetch in runCli is invoked as `globalThis.fetch`-equivalent. `as unknown as typeof fetch`
// at the boundary widens the structural mock; the production code only reads what we provide.
const asFetchOverride = (
  impl: (url: string, options?: { method?: string }) => Promise<ResponseLike>,
): typeof fetch => impl as unknown as typeof fetch;

// An interactive-terminal stdin so `login` takes the device flow (the path
// these URL-resolution cases drive) instead of the non-TTY direct-token
// resolve, which would fast-fail before any fetch.
const ttyStdin = { isTTY: true } as unknown as NodeJS.ReadableStream;

function makeFetchRecorder(): FetchRecorder {
  const calls: RecordedCall[] = [];
  const fetchImpl = async (
    url: string,
    options?: { method?: string },
  ): Promise<ResponseLike> => {
    calls.push({ url, method: options?.method ?? "GET" });
    if (calls.length === 1) {
      return {
        ok: true,
        status: 201,
        text: async () => JSON.stringify({
          session_id: "sess_test_url_resolution",
          login_url: "https://login.test/sess_test_url_resolution",
        }),
      };
    }
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ status: "expired" }),
    };
  };
  return { calls, fetchImpl };
}

describe("api url resolution drives every fetch from runCli", () => {
  test("dispatches the auth-session POST to the production default when no env or flag is set", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        ["login", "--no-open", "--no-input"],
        { stdout: out.stream, stderr: err.stream, stdin: ttyStdin, env: {}, fetchImpl: asFetchOverride(fetchImpl) },
      );
      // --no-input aborts at the verify prompt → exit 130, but only AFTER
      // createSession's POST has gone out to the resolved base URL (no poll).
      expect(code).toBe(130);
      expect(calls.length).toBeGreaterThan(0);
      expect(calls[0]).toEqual({ url: "https://api.agentsfleet.net/v1/auth/sessions", method: "POST" });
    });
  });

  test("honors AGENTSFLEET_API_URL env override at the fetch boundary", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        ["login", "--no-open", "--no-input"],
        {
          stdout: out.stream,
          stderr: err.stream,
          stdin: ttyStdin,
          env: { AGENTSFLEET_API_URL: "http://localhost:3000" },
          fetchImpl: asFetchOverride(fetchImpl),
        },
      );
      expect(code).toBe(130);
      expect(calls[0]).toEqual({ url: "http://localhost:3000/v1/auth/sessions", method: "POST" });
    });
  });

  test("honors --api flag over AGENTSFLEET_API_URL env at the fetch boundary", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        [
          "--api", "https://api-dev.agentsfleet.net",
          "login", "--no-open", "--no-input",
        ],
        {
          stdout: out.stream,
          stderr: err.stream,
          stdin: ttyStdin,
          env: { AGENTSFLEET_API_URL: "http://localhost:3000" },
          fetchImpl: asFetchOverride(fetchImpl),
        },
      );
      expect(code).toBe(130);
      expect(calls[0]).toEqual({ url: "https://api-dev.agentsfleet.net/v1/auth/sessions", method: "POST" });
    });
  });

  test("honors creds.api_url when no flag and no env override are set (regression: PR #297 review)", async () => {
    // Regression: a global arg parser used to bake DEFAULT_API_URL into its
    // return, making global.apiUrl always truthy and short-circuiting
    // creds.api_url. A user who ran `agentsfleet login --api http://localhost:3000`
    // would have their URL written to credentials.json but every subsequent
    // invocation silently fell through to the production default. Pre-seed
    // the saved api_url and prove it survives end-to-end as the ctx.apiUrl
    // that drives fetch.
    await withFreshStateDir(async () => {
      await saveCredentials({
        token: "header.payload.sig",
        saved_at: Date.now(),
        session_id: "sess_persisted",
        api_url: "http://localhost:3000",
      });

      const out = bufferStream();
      const err = bufferStream();
      const calls: { url: string }[] = [];
      const fetchImpl = async (url: string): Promise<ResponseLike> => {
        calls.push({ url });
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify({ status: "ok" }),
        };
      };

      await runCli(["doctor"], {
        stdout: out.stream,
        stderr: err.stream,
        env: {},
        fetchImpl: asFetchOverride(fetchImpl),
      });

      expect(calls.length).toBeGreaterThan(0);
      expect(calls[0]?.url).toBe("http://localhost:3000/healthz");
    });
  });

  // Full precedence-chain matrix. The bug fixed in bb1ca7c9 lived in the
  // cross-module short-circuit between the global-arg parser and creds
  // resolution — unit tests on the parser alone could not catch it. This
  // 16-case matrix walks every combination of (--api flag, AGENTSFLEET_API_URL
  // env, ambient unprefixed API_URL env, creds.api_url persisted) and asserts
  // the resolved URL drives the actual outbound fetch through runCli dispatch.
  //
  // `aenv` sets a bare API_URL. It is NOT a resolution source — the
  // unprefixed alias was removed as off-brand — so these cases double as the
  // regression guard that an ambient API_URL is ignored, never resolved.
  describe("full precedence matrix", () => {
    const FLAG = "https://flag.example";
    const ZENV = "https://agent-env.example";
    const AENV = "https://api-url-env.example";
    const CREDS = "https://saved-creds.example";
    const DEFAULT = "https://api.agentsfleet.net";

    interface MatrixCase {
      set: { flag: 0 | 1; zenv: 0 | 1; aenv: 0 | 1; creds: 0 | 1 };
      expected: string;
    }

    const cases: MatrixCase[] = [
      // Flag set → flag wins regardless of all four other inputs.
      { set: { flag: 1, zenv: 1, aenv: 1, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 1, aenv: 1, creds: 0 }, expected: FLAG },
      { set: { flag: 1, zenv: 1, aenv: 0, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 1, aenv: 0, creds: 0 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 1, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 1, creds: 0 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 0, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 0, creds: 0 }, expected: FLAG },
      // Flag unset, AGENTSFLEET_API_URL set → it wins; the ambient API_URL
      // alongside it is ignored either way.
      { set: { flag: 0, zenv: 1, aenv: 1, creds: 1 }, expected: ZENV },
      { set: { flag: 0, zenv: 1, aenv: 1, creds: 0 }, expected: ZENV },
      { set: { flag: 0, zenv: 1, aenv: 0, creds: 1 }, expected: ZENV },
      { set: { flag: 0, zenv: 1, aenv: 0, creds: 0 }, expected: ZENV },
      // Flag + AGENTSFLEET_API_URL unset, only a bare API_URL set → the alias
      // is ignored, so resolution falls through to creds.api_url, then DEFAULT.
      { set: { flag: 0, zenv: 0, aenv: 1, creds: 1 }, expected: CREDS },
      { set: { flag: 0, zenv: 0, aenv: 1, creds: 0 }, expected: DEFAULT },
      // Only creds set → creds.api_url wins over default. (This is the leg
      // the original bug short-circuited.)
      { set: { flag: 0, zenv: 0, aenv: 0, creds: 1 }, expected: CREDS },
      // Nothing explicit set → DEFAULT_API_URL.
      { set: { flag: 0, zenv: 0, aenv: 0, creds: 0 }, expected: DEFAULT },
    ];

    for (const c of cases) {
      const setNames = Object.entries(c.set)
        .filter(([, v]) => v === 1)
        .map(([k]) => k);
      const label = setNames.length === 0 ? "nothing set" : setNames.join(" + ");
      test(`${label} → ${c.expected}`, async () => {
        await withFreshStateDir(async () => {
          await saveCredentials({
            token: "header.payload.sig",
            saved_at: Date.now(),
            session_id: "sess_matrix",
            api_url: c.set.creds === 1 ? CREDS : null,
          });
          const env: NodeJS.ProcessEnv = {};
          if (c.set.zenv === 1) env.AGENTSFLEET_API_URL = ZENV;
          if (c.set.aenv === 1) env.API_URL = AENV;
          const argv = c.set.flag === 1 ? ["--api", FLAG, "doctor"] : ["doctor"];

          const out = bufferStream();
          const err = bufferStream();
          const calls: { url: string }[] = [];
          const fetchImpl = async (url: string): Promise<ResponseLike> => {
            calls.push({ url });
            return { ok: true, status: 200, text: async () => JSON.stringify({ status: "ok" }) };
          };

          await runCli(argv, {
            stdout: out.stream,
            stderr: err.stream,
            env,
            fetchImpl: asFetchOverride(fetchImpl),
          });
          expect(calls.length).toBeGreaterThan(0);
          expect(calls[0]?.url).toBe(`${c.expected}/healthz`);
        });
      });
    }
  });

  test("strips trailing slashes from --api before composing the request URL", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        [
          "--api", "https://api.agentsfleet.net//",
          "login", "--no-open", "--no-input",
        ],
        { stdout: out.stream, stderr: err.stream, stdin: ttyStdin, env: {}, fetchImpl: asFetchOverride(fetchImpl) },
      );
      expect(code).toBe(130);
      expect(calls[0]?.url).toBe("https://api.agentsfleet.net/v1/auth/sessions");
    });
  });
});
