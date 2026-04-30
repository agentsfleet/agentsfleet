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

// ── credential add/list/delete ─────────────────────────────────────────

test("credential add stores via API with structured data", async () => {
  let requestUrl = null;
  let requestMethod = null;
  let requestBody = null;
  const deps = makeDeps({
    request: async (_ctx, url, opts) => {
      requestUrl = url;
      requestMethod = opts.method;
      requestBody = JSON.parse(opts.body);
      return {};
    },
  });

  const code = await commandZombie(
    makeCtx(),
    ["credential", "add", "fly", '--data={"host":"api.machines.dev","api_token":"FLY_TOKEN"}'],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.equal(requestMethod, "POST");
  assert.ok(requestUrl.includes(`/v1/workspaces/${WS_ID}/credentials`));
  assert.equal(requestBody.name, "fly");
  assert.deepEqual(requestBody.data, { host: "api.machines.dev", api_token: "FLY_TOKEN" });
  assert.equal(requestBody.value, undefined);
});

test("credential add without name returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["credential", "add"], workspaces, makeDeps());
  assert.equal(code, 2);
});

test("credential add without --data returns exit 2", async () => {
  const code = await commandZombie(
    makeCtx(),
    ["credential", "add", "agentmail"],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 2);
});

test("credential add rejects invalid JSON", async () => {
  const code = await commandZombie(
    makeCtx(),
    ["credential", "add", "fly", "--data=not-json"],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 2);
});

test("credential add rejects non-object JSON (string)", async () => {
  const code = await commandZombie(
    makeCtx(),
    ["credential", "add", "fly", '--data="bare-string"'],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 2);
});

test("credential add rejects empty object", async () => {
  const code = await commandZombie(
    makeCtx(),
    ["credential", "add", "fly", "--data={}"],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 2);
});

test("credential list returns credentials", async () => {
  let printed = null;
  const deps = makeDeps({
    request: async () => ({ credentials: [{ name: "fly", created_at: "2026-04-26" }] }),
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

test("credential delete sends DELETE to per-credential URL", async () => {
  let requestUrl = null;
  let requestMethod = null;
  const deps = makeDeps({
    request: async (_ctx, url, opts) => {
      requestUrl = url;
      requestMethod = opts.method;
      return {};
    },
  });

  const code = await commandZombie(
    makeCtx(),
    ["credential", "delete", "fly"],
    workspaces,
    deps,
  );
  assert.equal(code, 0);
  assert.equal(requestMethod, "DELETE");
  assert.ok(requestUrl.endsWith(`/v1/workspaces/${WS_ID}/credentials/fly`));
});

test("credential delete without name returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["credential", "delete"], workspaces, makeDeps());
  assert.equal(code, 2);
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

test("kill sends PATCH /zombies/{id} with body status=killed", async () => {
  const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
  let requestMethod = null;
  let requestUrl = null;
  let requestBody = null;
  const deps = makeDeps({
    request: async (_ctx, url, opts) => {
      requestMethod = opts.method;
      requestUrl = url;
      requestBody = opts.body;
      return {};
    },
  });

  const code = await commandZombie(makeCtx(), ["kill", ZOMBIE_ID], workspaces, deps);
  assert.equal(code, 0);
  assert.equal(requestMethod, "PATCH");
  assert.ok(requestUrl.endsWith(`/zombies/${ZOMBIE_ID}`), `expected zombie path, got: ${requestUrl}`);
  assert.deepEqual(JSON.parse(requestBody), { status: "killed" });
});

test("kill without zombie_id returns MISSING_ARGUMENT", async () => {
  let errorCode = null;
  const deps = makeDeps({
    writeError: (_ctx, code) => {
      errorCode = code;
    },
  });
  const code = await commandZombie(makeCtx(), ["kill"], workspaces, deps);
  assert.equal(code, 2);
  assert.equal(errorCode, "MISSING_ARGUMENT");
});

// ── logs ───────────────────────────────────────────────────────────────

test("logs fetches per-zombie events stream (M42: activity → events repoint)", async () => {
  let requestUrl = null;
  const deps = makeDeps({
    request: async (_ctx, url) => {
      requestUrl = url;
      return { items: [{ actor: "webhook:github", status: "processed", response_text: "ok", created_at: 1745539200000 }] };
    },
  });

  const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";
  const code = await commandZombie(makeCtx(), ["logs", "--zombie", ZOMBIE_ID], workspaces, deps);
  assert.equal(code, 0);
  assert.ok(requestUrl.includes(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`));
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
