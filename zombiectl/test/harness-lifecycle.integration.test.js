import test from "node:test";
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
    reset: () => {
      data = "";
    },
  };
}

function parseJsonOutput(text) {
  return JSON.parse(String(text).trim());
}

test("harness lifecycle: activate deterministically changes subsequent run snapshot linkage", async () => {
  const out = bufferStream();
  const err = bufferStream();
  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-harness-lifecycle-"));
  const filePath = path.join(tmpDir, "profile.md");
  await fs.writeFile(filePath, "# Harness\n\n```json\n{\"profile_id\":\"ws_123-harness\",\"stages\":[]}\n```", "utf8");
  const prevStateDir = process.env.ZOMBIE_STATE_DIR;
  process.env.ZOMBIE_STATE_DIR = tmpDir;

  const state = {
    activeProfileVersion: null,
    profileByVersion: new Map(),
    sequence: 0,
    runCounter: 0,
  };

  const fetchImpl = async (url, options = {}) => {
    const u = new URL(url);
    const route = `${options.method || "GET"} ${u.pathname}`;

    if (route === "PUT /v1/workspaces/ws_123/harness/source") {
      state.sequence += 1;
      const version = `pver_${state.sequence}`;
      state.profileByVersion.set(version, "ws_123-harness");
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({ agent_id: "ws_123-harness", config_version_id: version }),
      };
    }

    if (route === "POST /v1/workspaces/ws_123/harness/compile") {
      const body = JSON.parse(String(options.body || "{}"));
      const version = body.config_version_id;
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({
          compile_job_id: `cjob_${version}`,
          agent_id: state.profileByVersion.get(version),
          config_version_id: version,
          is_valid: true,
          validation_report_json: "{}",
        }),
      };
    }

    if (route === "POST /v1/workspaces/ws_123/harness/activate") {
      const body = JSON.parse(String(options.body || "{}"));
      state.activeProfileVersion = body.config_version_id;
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({
          workspace_id: "ws_123",
          agent_id: state.profileByVersion.get(state.activeProfileVersion),
          config_version_id: state.activeProfileVersion,
          run_snapshot_version: state.activeProfileVersion,
          activated_by: body.activated_by || "zombiectl",
          activated_at: 1730000000,
        }),
      };
    }

    if (route === "GET /v1/workspaces/ws_123/harness/active") {
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({
          workspace_id: "ws_123",
          source: state.activeProfileVersion ? "active" : "default-v1",
          agent_id: state.activeProfileVersion ? state.profileByVersion.get(state.activeProfileVersion) : null,
          config_version_id: state.activeProfileVersion,
          run_snapshot_version: state.activeProfileVersion,
          active_at: state.activeProfileVersion ? 1730000000 : null,
          profile: { profile_id: state.activeProfileVersion || "default-v1", stages: [] },
        }),
      };
    }

    if (route === "POST /v1/runs") {
      state.runCounter += 1;
      return {
        ok: true,
        status: 202,
        text: async () => JSON.stringify({
          run_id: `r_${state.runCounter}`,
          state: "SPEC_QUEUED",
          attempt: 1,
          run_snapshot_version: state.activeProfileVersion,
          request_id: `req_${state.runCounter}`,
        }),
      };
    }

    throw new Error(`unexpected route: ${route}`);
  };

  try {
    const env = { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" };

    assert.equal(
      await runCli(["harness", "source", "put", "--workspace-id", "ws_123", "--file", filePath], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    assert.equal(
      await runCli(["harness", "compile", "--workspace-id", "ws_123", "--profile-version-id", "pver_1"], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    assert.equal(
      await runCli(["harness", "activate", "--workspace-id", "ws_123", "--profile-version-id", "pver_1"], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );

    out.reset();

    assert.equal(
      await runCli([
        "--json",
        "run",
        "--workspace-id",
        "ws_123",
        "--spec-id",
        "spec_123",
        "--idempotency-key",
        "idem_1",
      ], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    const firstRun = parseJsonOutput(out.read());
    assert.equal(firstRun.run_snapshot_version, "pver_1");

    out.reset();
    assert.equal(
      await runCli(["harness", "source", "put", "--workspace-id", "ws_123", "--file", filePath], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    assert.equal(
      await runCli(["harness", "compile", "--workspace-id", "ws_123", "--profile-version-id", "pver_2"], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    assert.equal(
      await runCli(["harness", "activate", "--workspace-id", "ws_123", "--profile-version-id", "pver_2"], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );

    out.reset();
    assert.equal(
      await runCli([
        "--json",
        "run",
        "--workspace-id",
        "ws_123",
        "--spec-id",
        "spec_123",
        "--idempotency-key",
        "idem_2",
      ], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    const secondRun = parseJsonOutput(out.read());

    assert.equal(secondRun.run_snapshot_version, "pver_2");
    assert.notEqual(firstRun.run_snapshot_version, secondRun.run_snapshot_version);
    assert.equal(err.read(), "");
  } finally {
    if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = prevStateDir;
    await fs.rm(tmpDir, { recursive: true, force: true });
  }
});

test("harness lifecycle contract: API and CLI JSON expose profile identity parity fields", async () => {
  const out = bufferStream();
  const err = bufferStream();
  const stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-harness-parity-"));
  const prevStateDir = process.env.ZOMBIE_STATE_DIR;
  process.env.ZOMBIE_STATE_DIR = stateDir;

  const fetchImpl = async (url, options = {}) => {
    const u = new URL(url);
    const route = `${options.method || "GET"} ${u.pathname}`;

    if (route === "POST /v1/workspaces/ws_123/harness/activate") {
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({
          workspace_id: "ws_123",
          agent_id: "ws_123-harness",
          config_version_id: "pver_2",
          run_snapshot_version: "pver_2",
          activated_by: "operator",
          activated_at: 1730000100,
        }),
      };
    }

    if (route === "GET /v1/workspaces/ws_123/harness/active") {
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({
          workspace_id: "ws_123",
          source: "active",
          agent_id: "ws_123-harness",
          config_version_id: "pver_2",
          run_snapshot_version: "pver_2",
          active_at: 1730000100,
          profile: { profile_id: "ws_123-harness", stages: [] },
        }),
      };
    }

    throw new Error(`unexpected route: ${route}`);
  };

  const env = { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" };

  try {
    assert.equal(
      await runCli(
        [
          "--json",
          "harness",
          "activate",
          "--workspace-id",
          "ws_123",
          "--profile-version-id",
          "pver_2",
          "--activated-by",
          "operator",
        ],
        {
          env,
          stdout: out.stream,
          stderr: err.stream,
          fetchImpl,
        },
      ),
      0,
    );
    const activatePayload = parseJsonOutput(out.read());
    assert.equal(activatePayload.agent_id, "ws_123-harness");
    assert.equal(activatePayload.config_version_id, "pver_2");
    assert.equal(activatePayload.run_snapshot_version, "pver_2");
    assert.equal(activatePayload.activated_at, 1730000100);

    out.reset();

    assert.equal(
      await runCli(["--json", "harness", "active", "--workspace-id", "ws_123"], {
        env,
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      }),
      0,
    );
    const activePayload = parseJsonOutput(out.read());
    assert.equal(activePayload.agent_id, "ws_123-harness");
    assert.equal(activePayload.config_version_id, "pver_2");
    assert.equal(activePayload.run_snapshot_version, "pver_2");
    assert.equal(activePayload.active_at, 1730000100);
    assert.equal(err.read(), "");
  } finally {
    if (prevStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = prevStateDir;
    await fs.rm(stateDir, { recursive: true, force: true });
  }
});
