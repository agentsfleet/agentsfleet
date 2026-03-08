import test from "node:test";
import assert from "node:assert/strict";
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

test("doctor --json emits machine-parseable payload", async () => {
  const out = bufferStream();
  const err = bufferStream();

  const fetchImpl = async (url) => {
    if (url.endsWith("/healthz")) {
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({ status: "ok", service: "zombied" }),
      };
    }
    if (url.endsWith("/readyz")) {
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({ ready: true }),
      };
    }
    return {
      ok: false,
      status: 404,
      statusText: "Not Found",
      text: async () => JSON.stringify({ error: { code: "NOT_FOUND", message: "Not found" } }),
    };
  };

  const code = await runCli(["--json", "doctor"], {
    env: { ...process.env, API_KEY: "dev-key" },
    stdout: out.stream,
    stderr: err.stream,
    fetchImpl,
  });

  assert.equal(code, 1);
  const parsed = JSON.parse(out.read());
  assert.equal(typeof parsed.ok, "boolean");
  assert.equal(Array.isArray(parsed.checks), true);
  assert.equal(parsed.checks.length >= 3, true);
});
