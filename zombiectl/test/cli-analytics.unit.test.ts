import { test } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.ts";
import {
  cliAnalytics,
  type AnalyticsClient,
} from "../src/lib/analytics.ts";
import { asFetchOverride, type ResponseLike } from "./helpers.ts";

interface TrackedEvent {
  client: AnalyticsClient | null;
  distinctId: string | null | undefined;
  event: string;
  properties: Record<string, unknown>;
}

function bufferStream(): { stream: Writable; read: () => string } {
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

function makeToken(payload: Record<string, unknown>): string {
  const header = Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url");
  const body = Buffer.from(JSON.stringify(payload)).toString("base64url");
  return `${header}.${body}.sig`;
}

async function withAnalyticsStub(fn: () => Promise<void>): Promise<void> {
  const originalCreate = cliAnalytics.createCliAnalytics;
  const originalTrack = cliAnalytics.trackCliEvent;
  const originalShutdown = cliAnalytics.shutdownCliAnalytics;

  try {
    await fn();
  } finally {
    cliAnalytics.createCliAnalytics = originalCreate;
    cliAnalytics.trackCliEvent = originalTrack;
    cliAnalytics.shutdownCliAnalytics = originalShutdown;
  }
}

async function withStateDir<T>(fn: (dir: string) => Promise<T>): Promise<T> {
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

// Pre-write session.json with known UUIDs so loadSession returns
// stable IDs. validUuid (state.ts) requires canonical UUID format —
// arbitrary strings get regenerated. These are fixed v4 fixtures.
const PINNED_DEVICE = "11111111-1111-4111-8111-111111111111";
const PINNED_SESSION = "22222222-2222-4222-8222-222222222222";
async function pinSession(dir: string): Promise<void> {
  await fs.writeFile(
    path.join(dir, "session.json"),
    JSON.stringify({ device_id: PINNED_DEVICE, session_id: PINNED_SESSION, last_activity: Date.now() }),
    { mode: 0o600 },
  );
}

test("runCli tracks login success with post-login distinct id and shuts down analytics", async () => {
  await withStateDir(async (dir) => {
    await pinSession(dir);
    await withAnalyticsStub(async () => {
      const events: TrackedEvent[] = [];
      let shutdownClient: AnalyticsClient | null | undefined = null;
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
      const fetchImpl = asFetchOverride(async (url, options): Promise<ResponseLike> => {
        if (options?.method === "POST") {
          return {
            ok: true,
            status: 201,
            statusText: "Created",
            headers: { get: () => null },
            text: async () => JSON.stringify({ session_id: "sess_analytics", login_url: "https://login.test" }),
          };
        }
        // After login completes the CLI hydrates the local workspace list
        // via GET /v1/tenants/me/workspaces — return an empty list so this
        // test stays focused on analytics events, not workspace shape.
        if (url.endsWith("/v1/tenants/me/workspaces")) {
          return {
            ok: true,
            status: 200,
            statusText: "OK",
            headers: { get: () => null },
            text: async () => JSON.stringify({ items: [], total: 0 }),
          };
        }
        pollCount += 1;
        return {
          ok: true,
          status: 200,
          statusText: "OK",
          headers: { get: () => null },
          text: async () => JSON.stringify({ status: "complete", token: clerkToken }),
        };
      });

      const code = await runCli(["login", "--no-open"], {
        env: { ...process.env, NO_COLOR: "1", BROWSER: "false" },
        stdout: stdout.stream,
        stderr: stderr.stream,
        fetchImpl,
      });

      assert.equal(code, 0);
      assert.equal(pollCount, 1);
      assert.equal(events.length, 4);
      assert.deepEqual(events.map(({ event }) => event), [
        "cli_command_started",
        "cli_command_finished",
        "user_authenticated",
        "login_completed",
      ]);
      assert.deepEqual(events[0], {
        client: analyticsClient,
        distinctId: "anonymous",
        event: "cli_command_started",
        properties: {
          command: "login",
          json_mode: "false",
          cli_session_id: PINNED_SESSION,
          cli_device_id: PINNED_DEVICE,
        },
      });
      // Wire keys are now namespaced: cli_session_id is the CLI
      // telemetry session (stable across the invocation), session_id is
      // the auth session set by login's setCliAnalyticsContext. Two
      // different concepts, two different keys — no collision.
      assert.deepEqual(events[1], {
        client: analyticsClient,
        distinctId: "anonymous",
        event: "cli_command_finished",
        properties: {
          command: "login",
          json_mode: "false",
          exit_code: "0",
          cli_session_id: PINNED_SESSION,
          cli_device_id: PINNED_DEVICE,
          session_id: "sess_analytics",
        },
      });
      assert.deepEqual(events[2], {
        client: analyticsClient,
        distinctId: "user_login_123",
        event: "user_authenticated",
        properties: { command: "login", session_id: "sess_analytics" },
      });
      assert.deepEqual(events[3], {
        client: analyticsClient,
        distinctId: "user_login_123",
        event: "login_completed",
        properties: { command: "login", session_id: "sess_analytics" },
      });
      assert.equal(shutdownClient, analyticsClient);
    });
  });
});

test("runCli tracks workspace creation with existing distinct id", async () => {
  await withStateDir(async (dir) => {
    await pinSession(dir);
    await withAnalyticsStub(async () => {
      const events: TrackedEvent[] = [];
      const analyticsClient = { name: "workspace-client" };
      const clerkToken = makeToken({ sub: "user_workspace_456" });

      cliAnalytics.createCliAnalytics = async () => analyticsClient;
      cliAnalytics.trackCliEvent = (client, distinctId, event, properties = {}) => {
        events.push({ client, distinctId, event, properties });
      };
      cliAnalytics.shutdownCliAnalytics = async () => {};

      const stdout = bufferStream();
      const stderr = bufferStream();
      const fetchImpl = asFetchOverride(async (): Promise<ResponseLike> => ({
        ok: true,
        status: 201,
        statusText: "Created",
        headers: { get: () => null },
        text: async () =>
          JSON.stringify({
            workspace_id: "ws_123456789abc",
            name: "jolly-harbor-482",
            request_id: "req_workspace",
          }),
      }));

      const code = await runCli(["workspace", "add"], {
        env: { ...process.env, NO_COLOR: "1", BROWSER: "false", ZOMBIE_TOKEN: clerkToken },
        stdout: stdout.stream,
        stderr: stderr.stream,
        fetchImpl,
      });

      assert.equal(code, 0);
      assert.equal(events.length, 4);
      assert.deepEqual(events.map(({ event }) => event), [
        "cli_command_started",
        "cli_command_finished",
        "workspace_created",
        "workspace_add_completed",
      ]);
      assert.deepEqual(events[0], {
        client: analyticsClient,
        distinctId: "user_workspace_456",
        event: "cli_command_started",
        properties: {
          command: "workspace.add",
          json_mode: "false",
          cli_session_id: PINNED_SESSION,
          cli_device_id: PINNED_DEVICE,
        },
      });
      assert.deepEqual(events[1], {
        client: analyticsClient,
        distinctId: "user_workspace_456",
        event: "cli_command_finished",
        properties: {
          command: "workspace.add",
          json_mode: "false",
          exit_code: "0",
          cli_session_id: PINNED_SESSION,
          cli_device_id: PINNED_DEVICE,
          workspace_id: "ws_123456789abc",
        },
      });
      assert.deepEqual(events[2], {
        client: analyticsClient,
        distinctId: "user_workspace_456",
        event: "workspace_created",
        properties: {
          command: "workspace.add",
          workspace_id: "ws_123456789abc",
        },
      });
      assert.deepEqual(events[3], {
        client: analyticsClient,
        distinctId: "user_workspace_456",
        event: "workspace_add_completed",
        properties: {
          command: "workspace.add",
          workspace_id: "ws_123456789abc",
        },
      });
    });
  });
});

test("runCli tracks unknown-command errors and still shuts down analytics when tracking throws", async () => {
  await withStateDir(async (dir) => {
    await pinSession(dir);
    await withAnalyticsStub(async () => {
      const events: Array<Omit<TrackedEvent, "client">> = [];
      let shutdownCalls = 0;
      interface CaptureArg {
        distinctId: string | null | undefined;
        event: string;
        properties: Record<string, unknown>;
      }
      const analyticsClient = {
        capture({ distinctId, event, properties }: CaptureArg) {
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
      assert.match(stderr.read(), /unknown command.*runx/);
      assert.equal(events.length, 1);
      assert.equal(events[0]?.event, "cli_error");
      assert.equal(events[0]?.distinctId, "anonymous");
      // GAP B: commander-level cli_error must carry the same base props
      // that runCommand's emits carry — otherwise PostHog loses session
      // correlation on unknown-command / usage-error paths.
      assert.equal(events[0]?.properties.cli_session_id, PINNED_SESSION);
      assert.equal(events[0]?.properties.cli_device_id, PINNED_DEVICE);
      assert.equal(shutdownCalls, 1);
    });
  });
});

test("runCli persists session.json with bumped last_activity before returning (fast-exit flush guarantee)", async () => {
  await withStateDir(async (dir) => {
    await withAnalyticsStub(async () => {
      cliAnalytics.createCliAnalytics = async () => null;
      cliAnalytics.trackCliEvent = () => {};
      cliAnalytics.shutdownCliAnalytics = async () => {};
      const stdout = bufferStream();
      const stderr = bufferStream();
      // No session.json pre-existing — loadSession generates fresh.
      const t0 = Date.now();
      // --help is the fast-exit path most likely to race process.exit
      // against a fire-and-forget saveSession. If the await regressed,
      // this assertion would catch it: session.json must exist with a
      // bumped last_activity by the time runCli returns.
      await runCli(["--help"], {
        env: { ...process.env, NO_COLOR: "1" },
        stdout: stdout.stream,
        stderr: stderr.stream,
      });
      const sessionPath = path.join(dir, "session.json");
      const body = await fs.readFile(sessionPath, "utf8");
      const parsed = JSON.parse(body) as { device_id: string; session_id: string; last_activity: number | null };
      assert.match(parsed.device_id, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);
      assert.match(parsed.session_id, /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i);
      assert.ok(typeof parsed.last_activity === "number" && parsed.last_activity >= t0, "last_activity must be bumped by runCli startup");
    });
  });
});

test("runCli UNEXPECTED-branch cli_error carries cli_session_id + cli_device_id base props", async () => {
  // GAP cover: cli.ts:325-338 — when parseAsync throws a non-Commander,
  // non-InvalidArgumentError, the catch emits cli_error with the same
  // namespaced base props that runCommand emits. Trigger by making the
  // cli_command_started track call throw a plain Error from inside
  // runCommand (line is outside runCommand's own try/catch), which
  // propagates up through parseAsync into cli.ts's outer catch.
  await withStateDir(async (dir) => {
    await pinSession(dir);
    await withAnalyticsStub(async () => {
      const events: Array<Omit<TrackedEvent, "client">> = [];
      const analyticsClient = { name: "test-client" };
      cliAnalytics.createCliAnalytics = async () => analyticsClient;
      cliAnalytics.trackCliEvent = (_client, distinctId, event, properties = {}) => {
        if (event === "cli_command_started") {
          throw new Error("simulated unexpected analytics fault");
        }
        events.push({ distinctId, event, properties });
      };
      cliAnalytics.shutdownCliAnalytics = async () => {};

      const stdout = bufferStream();
      const stderr = bufferStream();

      // login is AUTH_EXEMPT so preAction doesn't short-circuit; the
      // runCommand wrapper fires cli_command_started before any handler
      // logic, so we never reach the network. fetchImpl unused but
      // required for type.
      const code = await runCli(["login", "--no-open"], {
        env: { ...process.env, NO_COLOR: "1", BROWSER: "false" },
        stdout: stdout.stream,
        stderr: stderr.stream,
        fetchImpl: asFetchOverride(async () => {
          throw new Error("fetch should not be reached — cli_command_started throws first");
        }),
      });

      assert.equal(code, 1);
      assert.equal(events.length, 1);
      assert.equal(events[0]?.event, "cli_error");
      assert.equal(events[0]?.properties.error_code, "UNEXPECTED");
      assert.equal(events[0]?.properties.exit_code, "1");
      assert.equal(events[0]?.properties.cli_session_id, PINNED_SESSION);
      assert.equal(events[0]?.properties.cli_device_id, PINNED_DEVICE);
      assert.match(stderr.read(), /error: simulated unexpected analytics fault/);
    });
  });
});

test("runCli honors analytics opt-out with bundled key", async () => {
  await withAnalyticsStub(async () => {
    let createCalls = 0;
    let trackCalls = 0;
    let shutdownCalls = 0;

    cliAnalytics.createCliAnalytics = async (env = process.env) => {
      createCalls += 1;
      assert.equal(env.DISABLE_TELEMETRY, "1");
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
      env: { ...process.env, NO_COLOR: "1", DISABLE_TELEMETRY: "1" },
      stdout: stdout.stream,
      stderr: stderr.stream,
    });

    assert.equal(code, 2);
    assert.match(stderr.read(), /unknown command.*runx/);
    assert.equal(createCalls, 1);
    assert.equal(trackCalls, 0);
    assert.equal(shutdownCalls, 1);
  });
});
