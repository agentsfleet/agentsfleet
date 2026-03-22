import { test } from "bun:test";
import assert from "node:assert/strict";
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
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-doctor-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    await fs.writeFile(
      path.join(dir, "workspaces.json"),
      `${JSON.stringify({ current_workspace_id: "ws_test", items: [{ workspace_id: "ws_test" }] }, null, 2)}\n`,
      "utf8",
    );
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

test("doctor --json emits machine-parseable payload", async () => {
  await withStateDir(async () => {
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
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });

    assert.equal(code, 0);
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.ok, true);
    assert.equal(Array.isArray(parsed.checks), true);
    assert.equal(parsed.checks.length >= 4, true);
  });
});
