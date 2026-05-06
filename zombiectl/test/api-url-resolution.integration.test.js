import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";
import { saveCredentials } from "../src/lib/state.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({
      write(chunk, _enc, cb) {
        data += String(chunk);
        cb();
      },
    }),
    read: () => data,
  };
}

async function withStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-state-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

function makeFetchRecorder() {
  const calls = [];
  const fetchImpl = async (url, options) => {
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
    await withStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        ["login", "--no-open", "--no-input", "--timeout-sec", "1", "--poll-ms", "50"],
        { stdout: out.stream, stderr: err.stream, env: {}, fetchImpl },
      );
      expect(code).toBe(1);
      expect(calls.length).toBeGreaterThan(0);
      expect(calls[0]).toEqual({ url: "https://api.usezombie.com/v1/auth/sessions", method: "POST" });
      const sessionPoll = calls.find((c) => c.method === "GET");
      expect(sessionPoll?.url).toBe("https://api.usezombie.com/v1/auth/sessions/sess_test_url_resolution");
    });
  });

  test("honors ZOMBIE_API_URL env override at the fetch boundary", async () => {
    await withStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        ["login", "--no-open", "--no-input", "--timeout-sec", "1", "--poll-ms", "50"],
        {
          stdout: out.stream,
          stderr: err.stream,
          env: { ZOMBIE_API_URL: "http://localhost:3000" },
          fetchImpl,
        },
      );
      expect(code).toBe(1);
      expect(calls[0]).toEqual({ url: "http://localhost:3000/v1/auth/sessions", method: "POST" });
    });
  });

  test("honors --api flag over ZOMBIE_API_URL env at the fetch boundary", async () => {
    await withStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        [
          "--api", "https://api-dev.usezombie.com",
          "login", "--no-open", "--no-input", "--timeout-sec", "1", "--poll-ms", "50",
        ],
        {
          stdout: out.stream,
          stderr: err.stream,
          env: { ZOMBIE_API_URL: "http://localhost:3000" },
          fetchImpl,
        },
      );
      expect(code).toBe(1);
      expect(calls[0]).toEqual({ url: "https://api-dev.usezombie.com/v1/auth/sessions", method: "POST" });
    });
  });

  test("honors creds.api_url when no flag and no env override are set (regression: PR #297 review)", async () => {
    // Regression for the dead-code bug in cli.js:76's `||` chain: parseGlobalArgs
    // used to bake DEFAULT_API_URL into its return, making global.apiUrl always
    // truthy and short-circuiting creds.api_url. A user who ran
    // `zombiectl login --api http://localhost:3000` would have their URL written
    // to credentials.json but every subsequent invocation silently fell through
    // to the production default. This test pre-seeds the saved api_url and
    // proves it survives end-to-end as the ctx.apiUrl that drives fetch.
    await withStateDir(async () => {
      await saveCredentials({
        token: "header.payload.sig",
        saved_at: Date.now(),
        session_id: "sess_persisted",
        api_url: "http://localhost:3000",
      });

      const out = bufferStream();
      const err = bufferStream();
      const calls = [];
      const fetchImpl = async (url) => {
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
        fetchImpl,
      });

      expect(calls.length).toBeGreaterThan(0);
      expect(calls[0].url).toBe("http://localhost:3000/healthz");
    });
  });

  // Full precedence-chain matrix. The bug fixed in bb1ca7c9 lived in the
  // composition at cli.js:76 — parseGlobalArgs unit tests alone could not
  // catch it because the bug was the cross-module short-circuit, not the
  // parser. This 16-case matrix walks every combination of (--api flag,
  // ZOMBIE_API_URL env, API_URL env, creds.api_url persisted) and asserts
  // the resolved URL drives the actual outbound fetch through the runCli
  // dispatch.
  describe("full precedence matrix", () => {
    const FLAG = "https://flag.example";
    const ZENV = "https://zombie-env.example";
    const AENV = "https://api-url-env.example";
    const CREDS = "https://saved-creds.example";
    const DEFAULT = "https://api.usezombie.com";

    const cases = [
      // Flag set → flag wins regardless of all four other inputs.
      { set: { flag: 1, zenv: 1, aenv: 1, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 1, aenv: 1, creds: 0 }, expected: FLAG },
      { set: { flag: 1, zenv: 1, aenv: 0, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 1, aenv: 0, creds: 0 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 1, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 1, creds: 0 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 0, creds: 1 }, expected: FLAG },
      { set: { flag: 1, zenv: 0, aenv: 0, creds: 0 }, expected: FLAG },
      // Flag unset, ZOMBIE_API_URL set → ZOMBIE_API_URL wins over API_URL / creds / default.
      { set: { flag: 0, zenv: 1, aenv: 1, creds: 1 }, expected: ZENV },
      { set: { flag: 0, zenv: 1, aenv: 1, creds: 0 }, expected: ZENV },
      { set: { flag: 0, zenv: 1, aenv: 0, creds: 1 }, expected: ZENV },
      { set: { flag: 0, zenv: 1, aenv: 0, creds: 0 }, expected: ZENV },
      // Flag + ZOMBIE_API_URL unset, API_URL set → API_URL wins over creds / default.
      { set: { flag: 0, zenv: 0, aenv: 1, creds: 1 }, expected: AENV },
      { set: { flag: 0, zenv: 0, aenv: 1, creds: 0 }, expected: AENV },
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
        await withStateDir(async () => {
          await saveCredentials({
            token: "header.payload.sig",
            saved_at: Date.now(),
            session_id: "sess_matrix",
            api_url: c.set.creds === 1 ? CREDS : null,
          });
          const env = {};
          if (c.set.zenv === 1) env.ZOMBIE_API_URL = ZENV;
          if (c.set.aenv === 1) env.API_URL = AENV;
          const argv = c.set.flag === 1 ? ["--api", FLAG, "doctor"] : ["doctor"];

          const out = bufferStream();
          const err = bufferStream();
          const calls = [];
          const fetchImpl = async (url) => {
            calls.push({ url });
            return { ok: true, status: 200, text: async () => JSON.stringify({ status: "ok" }) };
          };

          await runCli(argv, { stdout: out.stream, stderr: err.stream, env, fetchImpl });
          expect(calls.length).toBeGreaterThan(0);
          expect(calls[0].url).toBe(`${c.expected}/healthz`);
        });
      });
    }
  });

  test("strips trailing slashes from --api before composing the request URL", async () => {
    await withStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const { calls, fetchImpl } = makeFetchRecorder();
      const code = await runCli(
        [
          "--api", "https://api.usezombie.com//",
          "login", "--no-open", "--no-input", "--timeout-sec", "1", "--poll-ms", "50",
        ],
        { stdout: out.stream, stderr: err.stream, env: {}, fetchImpl },
      );
      expect(code).toBe(1);
      expect(calls[0].url).toBe("https://api.usezombie.com/v1/auth/sessions");
    });
  });
});
