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

test("harness compile sends profile_id selector", async () => {
  const out = bufferStream();
  const err = bufferStream();

  const fetchImpl = async (url, options) => {
    assert.equal(url, "http://localhost:3000/v1/workspaces/ws_123/harness/compile");
    assert.equal(options.method, "POST");
    const payload = JSON.parse(String(options.body));
    assert.equal(payload.profile_id, "ws_123-harness");
    assert.equal(payload.profile_version_id, null);
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ compile_job_id: "cjob_123", is_valid: true }),
    };
  };

  const code = await runCli(
    ["harness", "compile", "--workspace-id", "ws_123", "--profile-id", "ws_123-harness"],
    {
      env: { ...process.env, API_KEY: "dev-key" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    },
  );

  assert.equal(code, 0);
  assert.equal(err.read(), "");
  assert.match(out.read(), /compile_job_id=cjob_123/);
});

test("harness compile sends profile_version_id selector", async () => {
  const out = bufferStream();
  const err = bufferStream();

  const fetchImpl = async (_url, options) => {
    const payload = JSON.parse(String(options.body));
    assert.equal(payload.profile_id, null);
    assert.equal(payload.profile_version_id, "pver_456");
    return {
      ok: true,
      status: 200,
      text: async () => JSON.stringify({ compile_job_id: "cjob_456", is_valid: true }),
    };
  };

  const code = await runCli(
    ["harness", "compile", "--workspace-id", "ws_123", "--profile-version-id", "pver_456"],
    {
      env: { ...process.env, API_KEY: "dev-key" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    },
  );

  assert.equal(code, 0);
  assert.equal(err.read(), "");
  assert.match(out.read(), /compile_job_id=cjob_456/);
});
