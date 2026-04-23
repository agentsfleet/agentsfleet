// Zombie CLI unit tests — covers non-install/up subcommands.
//
// The install/up coverage that used to live here was retired when those
// commands collapsed into `zombiectl install --from <path>`. That flow's
// dims live in `zombie-install-from-path.unit.test.js`.
//
// Covers: credential add/list, status, kill, logs, unknown subcommand.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandZombie } from "../src/commands/zombie.js";
import { makeNoop, ui, WS_ID } from "./helpers.js";
import { parseFlags } from "../src/program/args.js";

function makeDeps(overrides = {}) {
  return {
    parseFlags,
    request: async () => ({}),
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    printKeyValue: () => {},
    printSection: () => {},
    writeLine: () => {},
    writeError: () => {},
    ...overrides,
  };
}

function makeCtx(overrides = {}) {
  return {
    stdout: makeNoop(),
    stderr: makeNoop(),
    jsonMode: false,
    noInput: false,
    ...overrides,
  };
}

const workspaces = { current_workspace_id: WS_ID, items: [] };

// ── credential add/list ────────────────────────────────────────────────

test("credential add stores via API", async () => {
  let requestUrl = null;
  let requestBody = null;
  const deps = makeDeps({
    request: async (_ctx, url, opts) => {
      requestUrl = url;
      requestBody = JSON.parse(opts.body);
      return {};
    },
  });

  const code = await commandZombie(
    makeCtx(),
    ["credential", "add", "agentmail", "--value=sk-test-123"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.ok(requestUrl.includes(`/v1/workspaces/${WS_ID}/credentials`));
  assert.equal(requestBody.name, "agentmail");
  assert.equal(requestBody.value, "sk-test-123");
  assert.equal(requestBody.workspace_id, undefined);
});

test("credential add without name returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["credential", "add"], workspaces, makeDeps());
  assert.equal(code, 2);
});

test("credential add without value in no-input mode returns exit 1", async () => {
  const code = await commandZombie(
    makeCtx({ noInput: true }),
    ["credential", "add", "agentmail"],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 1);
});

test("credential list returns credentials", async () => {
  let printed = null;
  const deps = makeDeps({
    request: async () => ({ credentials: [{ name: "agentmail", created_at: "2026-04-08" }] }),
    printJson: (_s, v) => { printed = v; },
  });

  const code = await commandZombie(
    makeCtx({ jsonMode: true }),
    ["credential", "list"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.ok(printed.credentials.length > 0);
});

// ── status ────────────────────────────────────────────────────────────

test("status shows zombie info", async () => {
  let printed = null;
  const deps = makeDeps({
    request: async () => ({
      zombies: [{
        name: "lead-collector",
        status: "active",
        events_processed: 42,
        budget_used_dollars: 1.23,
      }],
    }),
    printJson: (_s, v) => { printed = v; },
  });

  const code = await commandZombie(
    makeCtx({ jsonMode: true }),
    ["status"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.equal(printed.zombies[0].name, "lead-collector");
});

test("status with no zombies shows info message", async () => {
  const deps = makeDeps({
    request: async () => ({ zombies: [] }),
  });

  const code = await commandZombie(makeCtx(), ["status"], workspaces, deps);
  assert.equal(code, 0);
});

test("status without workspace returns exit 1", async () => {
  const code = await commandZombie(makeCtx(), ["status"], { current_workspace_id: null }, makeDeps());
  assert.equal(code, 1);
});

// ── kill ───────────────────────────────────────────────────────────────

test("kill sends DELETE to API", async () => {
  let requestMethod = null;
  const deps = makeDeps({
    request: async (_ctx, _url, opts) => {
      requestMethod = opts.method;
      return {};
    },
  });

  const code = await commandZombie(makeCtx(), ["kill"], workspaces, deps);
  assert.equal(code, 0);
  assert.equal(requestMethod, "DELETE");
});

// ── logs ───────────────────────────────────────────────────────────────

test("logs fetches per-zombie activity stream", async () => {
  let requestUrl = null;
  const deps = makeDeps({
    request: async (_ctx, url) => {
      requestUrl = url;
      return { events: [{ event_type: "webhook_received", detail: "evt_001" }] };
    },
  });

  const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
  const code = await commandZombie(makeCtx(), ["logs", "--zombie", ZOMBIE_ID], workspaces, deps);
  assert.equal(code, 0);
  assert.ok(requestUrl.includes(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/activity`));
});

test("logs without --zombie returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["logs"], workspaces, makeDeps());
  assert.equal(code, 2);
});

// ── unknown subcommand ─────────────────────────────────────────────────

test("unknown subcommand returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["badcmd"], workspaces, makeDeps());
  assert.equal(code, 2);
});

test("no subcommand returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), [], workspaces, makeDeps());
  assert.equal(code, 2);
});
