// M2_002 — Zombie CLI command unit tests.
//
// Covers spec dimensions:
//   1.1 SKILL.md template exists with ClaHub frontmatter
//   1.2 TRIGGER.md template exists with trigger config
//   2.1 zombiectl install creates directory with both files
//   2.2 zombiectl up reads both files, sends raw to API
//   2.3 zombiectl up with no zombie directory returns error
//   2.4 simpleYamlParse deleted (grep check, not test)
//   5.3 zombiectl credential add (mocked API)
//   5.4 zombiectl status (mocked API)

import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandZombie } from "../src/commands/zombie.js";
import { makeNoop, ui, WS_ID } from "./helpers.js";
import { parseFlags } from "../src/program/args.js";
import { tmpdir } from "node:os";
import { mkdtempSync, readFileSync, existsSync } from "node:fs";
import { join } from "node:path";

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

// ── 2.1: install creates directory ──────────────────────────────────────

test("2.1: install lead-collector creates directory with SKILL.md and TRIGGER.md", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);
  try {
    const code = await commandZombie(makeCtx(), ["install", "lead-collector"], workspaces, makeDeps());
    assert.equal(code, 0);
    assert.ok(existsSync(join(tmpDir, "lead-collector", "SKILL.md")));
    assert.ok(existsSync(join(tmpDir, "lead-collector", "TRIGGER.md")));
  } finally {
    process.chdir(origCwd);
  }
});

test("1.1: SKILL.md contains ClaHub frontmatter", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);
  try {
    await commandZombie(makeCtx(), ["install", "lead-collector"], workspaces, makeDeps());
    const content = readFileSync(join(tmpDir, "lead-collector", "SKILL.md"), "utf-8");
    assert.ok(content.startsWith("---"));
    assert.ok(content.includes("name: lead-collector"));
    assert.ok(content.includes("description:"));
    assert.ok(content.includes("tags:"));
    assert.ok(content.includes("author:"));
    assert.ok(content.includes("version:"));
  } finally {
    process.chdir(origCwd);
  }
});

test("1.2: TRIGGER.md contains trigger config", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);
  try {
    await commandZombie(makeCtx(), ["install", "lead-collector"], workspaces, makeDeps());
    const content = readFileSync(join(tmpDir, "lead-collector", "TRIGGER.md"), "utf-8");
    assert.ok(content.startsWith("---"));
    assert.ok(content.includes("trigger:"));
    assert.ok(content.includes("credentials:"));
    assert.ok(content.includes("budget:"));
    assert.ok(content.includes("chain:"));
  } finally {
    process.chdir(origCwd);
  }
});

test("2.1: install with --json outputs JSON", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);
  let printed = null;
  try {
    const code = await commandZombie(
      makeCtx({ jsonMode: true }),
      ["install", "lead-collector"],
      workspaces,
      makeDeps({ printJson: (_s, v) => { printed = v; } }),
    );
    assert.equal(code, 0);
    assert.equal(printed.status, "installed");
    assert.equal(printed.template, "lead-collector");
  } finally {
    process.chdir(origCwd);
  }
});

test("2.1: install unknown template returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["install", "nonexistent"], workspaces, makeDeps());
  assert.equal(code, 2);
});

test("2.1: install without template name returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["install"], workspaces, makeDeps());
  assert.equal(code, 2);
});

// ── 2.2: up sends both files raw ────────────────────────────────────────

test("2.2: up sends source_markdown and trigger_markdown to API", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);

  // Install first
  await commandZombie(makeCtx(), ["install", "lead-collector"], workspaces, makeDeps());

  let requestUrl = null;
  let requestBody = null;
  const deps = makeDeps({
    request: async (_ctx, url, opts) => {
      requestUrl = url;
      requestBody = JSON.parse(opts.body);
      return { zombie_id: "z-123", status: "active" };
    },
  });

  try {
    const code = await commandZombie(makeCtx(), ["up"], workspaces, deps);
    assert.equal(code, 0);
    assert.ok(requestUrl.includes("/v1/zombies/"));
    // M2_002: no config_json — server parses trigger_markdown
    assert.ok(requestBody.source_markdown);
    assert.ok(requestBody.trigger_markdown);
    assert.equal(requestBody.config_json, undefined);
    assert.equal(requestBody.workspace_id, WS_ID);
  } finally {
    process.chdir(origCwd);
  }
});

test("2.3: up without zombie directory returns exit 1", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);
  try {
    const code = await commandZombie(makeCtx(), ["up"], workspaces, makeDeps());
    assert.equal(code, 1);
  } finally {
    process.chdir(origCwd);
  }
});

test("2.2: up without workspace returns exit 1", async () => {
  const tmpDir = mkdtempSync(join(tmpdir(), "zombie-test-"));
  const origCwd = process.cwd();
  process.chdir(tmpDir);
  await commandZombie(makeCtx(), ["install", "lead-collector"], { current_workspace_id: null }, makeDeps());
  try {
    const code = await commandZombie(makeCtx(), ["up"], { current_workspace_id: null }, makeDeps());
    assert.equal(code, 1);
  } finally {
    process.chdir(origCwd);
  }
});

// ── 5.3: credential add ────────────────────────────────────────────────

test("5.3: credential add stores via API", async () => {
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
  assert.ok(requestUrl.includes("/v1/zombies/credentials"));
  assert.equal(requestBody.name, "agentmail");
  assert.equal(requestBody.value, "sk-test-123");
});

test("5.3: credential add without name returns exit 2", async () => {
  const code = await commandZombie(makeCtx(), ["credential", "add"], workspaces, makeDeps());
  assert.equal(code, 2);
});

test("5.3: credential add without value in no-input mode returns exit 1", async () => {
  const code = await commandZombie(
    makeCtx({ noInput: true }),
    ["credential", "add", "agentmail"],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 1);
});

test("5.3: credential list returns credentials", async () => {
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

// ── 5.4: status ────────────────────────────────────────────────────────

test("5.4: status shows zombie info", async () => {
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

test("5.4: status with no zombies shows info message", async () => {
  const deps = makeDeps({
    request: async () => ({ zombies: [] }),
  });

  const code = await commandZombie(makeCtx(), ["status"], workspaces, deps);
  assert.equal(code, 0);
});

test("5.4: status without workspace returns exit 1", async () => {
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

test("logs fetches activity stream", async () => {
  let requestUrl = null;
  const deps = makeDeps({
    request: async (_ctx, url) => {
      requestUrl = url;
      return { events: [{ event_type: "webhook_received", detail: "evt_001" }] };
    },
  });

  const code = await commandZombie(makeCtx(), ["logs"], workspaces, deps);
  assert.equal(code, 0);
  assert.ok(requestUrl.includes("/v1/zombies/activity"));
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
