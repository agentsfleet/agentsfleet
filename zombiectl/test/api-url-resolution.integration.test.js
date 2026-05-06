import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";

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
