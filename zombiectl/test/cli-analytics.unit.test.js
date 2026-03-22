import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.js";
import { cliAnalytics } from "../src/lib/analytics.js";

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

function makeToken(payload) {
  const header = Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  return `${header}.${body}.sig`;
}

function withAnalyticsStub(fn) {
  const originalCreate = cliAnalytics.createCliAnalytics;
  const originalTrack = cliAnalytics.trackCliEvent;
  const originalShutdown = cliAnalytics.shutdownCliAnalytics;

  return (async () => {
    try {
      await fn();
    } finally {
      cliAnalytics.createCliAnalytics = originalCreate;
      cliAnalytics.trackCliEvent = originalTrack;
      cliAnalytics.shutdownCliAnalytics = originalShutdown;
    }
  })();
}

async function withStateDir(fn) {
  const previous = process.env.ZOMBIE_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-analytics-"));
  process.env.ZOMBIE_STATE_DIR = dir;
  try {
    return await fn(dir);
  } finally {
    if (previous === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

test("runCli tracks login success with post-login distinct id and shuts down analytics", async () => {
  await withStateDir(async () => {
    await withAnalyticsStub(async () => {
      const events = [];
      let shutdownClient = null;
      const analyticsClient = { name: "test-client" };
      const clerkToken = makeToken({ sub: "user_login_123" });

      cliAnalytics.createCliAnalytics = async () => analyticsClient;
      cliAnalytics.trackCliEvent = (client, distinctId, event, properties = {}) => {
        events.push({ client, distinctId, event, properties });
      };
      cliAnalytics.shutdownCliAnalytics = async (client) => {
        shutdownClient = client;
      };

      const stdout = bufferStream();
      const stderr = bufferStream();
      let pollCount = 0;
      const fetchImpl = async (_url, options = {}) => {
        if (options.method === "POST") {
          return {
            ok: true,
            status: 201,
            text: async () => JSON.stringify({ session_id: "sess_analytics", login_url: "https://login.test" }),
          };
        }
        pollCount += 1;
        return {
          ok: true,
          status: 200,
          text: async () => JSON.stringify({ status: "complete", token: clerkToken }),
        };
      };

      const code = await runCli(["login", "--no-open"], {
        env: { ...process.env, NO_COLOR: "1", BROWSER: "false" },
        stdout: stdout.stream,
        stderr: stderr.stream,
        fetchImpl,
      });

      assert.equal(code, 0);
      assert.equal(pollCount, 1);
      assert.equal(events.length, 3);
      assert.deepEqual(
        events.map(({ event, distinctId, properties }) => ({ event, distinctId, properties })),
        [
          {
            event: "cli_command_started",
            distinctId: null,
            properties: { command: "login", json_mode: "false" },
          },
          {
            event: "cli_command_finished",
            distinctId: null,
            properties: { command: "login", exit_code: "0" },
          },
          {
            event: "user_authenticated",
            distinctId: "user_login_123",
            properties: { command: "login" },
          },
        ],
      );
      assert.equal(shutdownClient, analyticsClient);
    });
  });
});

test("runCli tracks workspace creation with existing distinct id", async () => {
  await withStateDir(async () => {
    await withAnalyticsStub(async () => {
      const events = [];
      const analyticsClient = { name: "workspace-client" };
      const clerkToken = makeToken({ sub: "user_workspace_456" });

      cliAnalytics.createCliAnalytics = async () => analyticsClient;
      cliAnalytics.trackCliEvent = (client, distinctId, event, properties = {}) => {
        events.push({ client, distinctId, event, properties });
      };
      cliAnalytics.shutdownCliAnalytics = async () => {};

      const stdout = bufferStream();
      const stderr = bufferStream();
      const fetchImpl = async () => ({
        ok: true,
        status: 201,
        text: async () =>
          JSON.stringify({
            workspace_id: "ws_123456789abc",
            repo_url: "https://github.com/acme/repo",
            default_branch: "main",
            install_url: "https://github.com/apps/usezombie/installations/new?state=ws_123456789abc",
            request_id: "req_workspace",
          }),
      });

      const code = await runCli(["workspace", "add", "https://github.com/acme/repo"], {
        env: { ...process.env, NO_COLOR: "1", BROWSER: "false", ZOMBIE_TOKEN: clerkToken },
        stdout: stdout.stream,
        stderr: stderr.stream,
        fetchImpl,
      });

      assert.equal(code, 0);
      assert.deepEqual(
        events.map(({ event, distinctId, properties }) => ({ event, distinctId, properties })),
        [
          {
            event: "cli_command_started",
            distinctId: "user_workspace_456",
            properties: { command: "workspace", json_mode: "false" },
          },
          {
            event: "cli_command_finished",
            distinctId: "user_workspace_456",
            properties: { command: "workspace", exit_code: "0" },
          },
          {
            event: "workspace_created",
            distinctId: "user_workspace_456",
            properties: { command: "workspace" },
          },
        ],
      );
    });
  });
});

test("runCli tracks unknown-command errors and still shuts down analytics when tracking throws", async () => {
  await withStateDir(async () => {
    await withAnalyticsStub(async () => {
      const events = [];
      let shutdownCalls = 0;
      const analyticsClient = {
        capture({ distinctId, event, properties }) {
          events.push({ distinctId, event, properties });
          throw new Error("capture failed");
        },
      };

      cliAnalytics.createCliAnalytics = async () => analyticsClient;
      cliAnalytics.shutdownCliAnalytics = async () => {
        shutdownCalls += 1;
      };

      const stdout = bufferStream();
      const stderr = bufferStream();

      const code = await runCli(["runx"], {
        env: { ...process.env, NO_COLOR: "1" },
        stdout: stdout.stream,
        stderr: stderr.stream,
      });

      assert.equal(code, 2);
      assert.match(stderr.read(), /unknown command: runx/);
      assert.equal(events.length, 1);
      assert.equal(events[0].event, "cli_error");
      assert.equal(events[0].distinctId, "anonymous");
      assert.equal(shutdownCalls, 1);
    });
  });
});

test("runCli honors analytics opt-out with bundled key", async () => {
  await withAnalyticsStub(async () => {
    let createCalls = 0;
    let trackCalls = 0;
    let shutdownCalls = 0;

    cliAnalytics.createCliAnalytics = async (env) => {
      createCalls += 1;
      assert.equal(env.ZOMBIE_POSTHOG_ENABLED, "false");
      return null;
    };
    cliAnalytics.trackCliEvent = (client) => {
      if (!client) return;
      trackCalls += 1;
    };
    cliAnalytics.shutdownCliAnalytics = async () => {
      shutdownCalls += 1;
    };

    const stdout = bufferStream();
    const stderr = bufferStream();

    const code = await runCli(["runx"], {
      env: { ...process.env, NO_COLOR: "1", ZOMBIE_POSTHOG_ENABLED: "false" },
      stdout: stdout.stream,
      stderr: stderr.stream,
    });

    assert.equal(code, 2);
    assert.match(stderr.read(), /unknown command: runx/);
    assert.equal(createCalls, 1);
    assert.equal(trackCalls, 0);
    assert.equal(shutdownCalls, 1);
  });
});
