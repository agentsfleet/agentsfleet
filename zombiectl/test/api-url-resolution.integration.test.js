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
