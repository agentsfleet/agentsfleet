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

async function withStateDir(opts, fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-doctor-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    if (opts.workspace) {
      await fs.writeFile(
        path.join(dir, "workspaces.json"),
        `${JSON.stringify({ current_workspace_id: opts.workspace, items: [{ workspace_id: opts.workspace }] }, null, 2)}\n`,
        "utf8",
      );
    }
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

function jsonResponse(body, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    statusText: status === 200 ? "OK" : "ERR",
    text: async () => JSON.stringify(body),
  };
}

test("doctor --json: all checks pass → ok=true, exit 0, 3 named checks", async () => {
  await withStateDir({ workspace: "ws_test" }, async () => {
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = async (url) => {
      if (url.endsWith("/healthz")) return jsonResponse({ status: "ok", service: "zombied" });
      if (url.includes("/v1/workspaces/ws_test/zombies")) return jsonResponse({ items: [] });
      return jsonResponse({ error: { code: "NOT_FOUND" } }, 404);
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
    assert.equal(typeof parsed.api_url, "string");
    const names = parsed.checks.map((c) => c.name);
    assert.deepEqual(names, ["server_reachable", "workspace_selected", "workspace_binding_valid"]);
    assert.equal(parsed.checks.every((c) => c.ok === true), true);
  });
});

test("doctor --json: server unreachable → server_reachable false, exit 1", async () => {
  await withStateDir({ workspace: "ws_test" }, async () => {
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = async () => {
      throw new Error("connect ECONNREFUSED");
    };
    const code = await runCli(["--json", "doctor"], {
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    assert.equal(code, 1);
    const parsed = JSON.parse(out.read());
    assert.equal(parsed.ok, false);
    const reachable = parsed.checks.find((c) => c.name === "server_reachable");
    assert.equal(reachable.ok, false);
    assert.match(reachable.detail, /ECONNREFUSED/);
  });
});

test("doctor --json: no workspace selected → workspace_selected false, binding skipped", async () => {
  await withStateDir({}, async () => {
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = async (url) => {
      if (url.endsWith("/healthz")) return jsonResponse({ status: "ok" });
      throw new Error("unexpected request: " + url);
    };
    const code = await runCli(["--json", "doctor"], {
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    assert.equal(code, 1);
    const parsed = JSON.parse(out.read());
    const selected = parsed.checks.find((c) => c.name === "workspace_selected");
    const binding = parsed.checks.find((c) => c.name === "workspace_binding_valid");
    assert.equal(selected.ok, false);
    assert.equal(binding.ok, false);
    assert.match(binding.detail, /skipped/);
  });
});

test("doctor --json: token bound to wrong workspace → binding false, exit 1", async () => {
  await withStateDir({ workspace: "ws_test" }, async () => {
    const out = bufferStream();
    const err = bufferStream();
    const fetchImpl = async (url) => {
      if (url.endsWith("/healthz")) return jsonResponse({ status: "ok" });
      if (url.includes("/v1/workspaces/ws_test/zombies")) {
        return jsonResponse({ error: { code: "FORBIDDEN", message: "Workspace access denied" } }, 403);
      }
      throw new Error("unexpected request: " + url);
    };
    const code = await runCli(["--json", "doctor"], {
      env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    assert.equal(code, 1);
    const parsed = JSON.parse(out.read());
    const binding = parsed.checks.find((c) => c.name === "workspace_binding_valid");
    assert.equal(binding.ok, false);
    assert.match(binding.detail, /workspace list/);
  });
});

test("doctor without local auth → AUTH_REQUIRED before any HTTP", async () => {
  await withStateDir({ workspace: "ws_test" }, async () => {
    const out = bufferStream();
    const err = bufferStream();
    let fetchCalls = 0;
    const fetchImpl = async () => {
      fetchCalls += 1;
      throw new Error("should not be called");
    };
    const env = { ...process.env };
    delete env.ZOMBIE_TOKEN;
    delete env.API_KEY;
    delete env.ZOMBIE_API_KEY;
    const code = await runCli(["--json", "doctor"], {
      env,
      stdout: out.stream,
      stderr: err.stream,
      fetchImpl,
    });
    assert.equal(code, 1);
    assert.equal(fetchCalls, 0);
    const parsed = JSON.parse(err.read());
    assert.equal(parsed.error.code, "AUTH_REQUIRED");
  });
});
