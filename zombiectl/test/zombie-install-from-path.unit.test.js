// `zombiectl install --from <path>` — unified install command.
//
// Post-server-side-parse: the CLI sends raw {trigger_markdown, source_markdown}
// and trusts the server's response for the canonical name. CLI-side YAML
// frontmatter parsing was removed (it was a duplicated parser; the server's
// is the source of truth). These tests cover: shape of the POST body,
// display-name precedence (server response > directory basename fallback),
// loader errors (path/file/skill/trigger missing), pre-flight (workspace,
// auth bubbling), and error surfacing (ApiError vs IO_ERROR).

import { test } from "bun:test";
import assert from "node:assert/strict";
import { tmpdir } from "node:os";
import { mkdtempSync, writeFileSync, mkdirSync, existsSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PKG_ROOT = dirname(TEST_DIR);

import { commandZombie } from "../src/commands/zombie.js";
import { findRoute } from "../src/program/routes.js";
import { parseFlags } from "../src/program/args.js";
import { makeNoop, ui, WS_ID } from "./helpers.js";

function makeStdout() {
  const lines = [];
  return {
    write: (s) => lines.push(s),
    lines,
  };
}

function makeDeps(overrides = {}) {
  return {
    parseFlags,
    // Default mock mirrors what the create handler returns post-parse.
    // Tests that need a different shape override `request`.
    request: async () => ({
      zombie_id: "zom_01abc",
      name: "test-zombie",
      status: "active",
    }),
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    printKeyValue: () => {},
    printSection: () => {},
    writeLine: (stream, line = "") => stream.write(line + "\n"),
    writeError: () => {},
    ...overrides,
  };
}

function setupSampleDir({ name = "test-zombie", withSkill = true, withTrigger = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const sampleDir = join(root, name);
  mkdirSync(sampleDir);
  if (withSkill) {
    writeFileSync(join(sampleDir, "SKILL.md"), `---\nname: ${name}\n---\n# skill body\n`);
  }
  if (withTrigger) {
    writeFileSync(
      join(sampleDir, "TRIGGER.md"),
      `---\nname: ${name}\ntrigger:\n  type: api\ntools:\n  - agentmail\nbudget:\n  daily_dollars: 1.0\n---\n# trigger body\n`,
    );
  }
  return { root, sampleDir };
}

const workspaces = { current_workspace_id: WS_ID, items: [] };
const noWorkspace = { current_workspace_id: null, items: [] };

// ── §1 — Loader + happy path ──────────────────────────────────────────────

test("install --from: happy path prints 'is live' from server-returned name", async () => {
  const { sampleDir } = setupSampleDir();
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("test-zombie is live."), `expected 'is live' line:\n${out}`);
  assert.ok(!out.includes("🎉"), `no party emoji per design system:\n${out}`);
  assert.ok(out.includes("Zombie ID: zom_01abc"), `expected zombie ID line:\n${out}`);
});

test("install --from: server-returned name wins over directory basename", async () => {
  // The server parses TRIGGER.md frontmatter — its name is canonical.
  // Directory basename is only a fallback for when the server omits it.
  const { sampleDir } = setupSampleDir({ name: "directory-basename" });
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async () => ({
        zombie_id: "zom_01abc",
        name: "frontmatter-name",
        status: "active",
      }),
    }),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("frontmatter-name is live."), `server name should win:\n${out}`);
  assert.ok(!out.includes("directory-basename"), `basename must not appear when server returned a name:\n${out}`);
});

test("install --from: server omits name → CLI falls back to directory basename", async () => {
  const { sampleDir } = setupSampleDir({ name: "fallback-zombie" });
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async () => ({ zombie_id: "zom_01abc", status: "active" }),
    }),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("fallback-zombie is live."), `expected basename fallback:\n${out}`);
});

test("install --from: missing directory exits 1 with ERR_PATH_NOT_FOUND", async () => {
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", "/nonexistent/install-from-test-path"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured?.code, "ERR_PATH_NOT_FOUND");
});

test("install --from: only SKILL.md present exits 1 with ERR_TRIGGER_MISSING", async () => {
  const { sampleDir } = setupSampleDir({ withTrigger: false });
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured?.code, "ERR_TRIGGER_MISSING");
  assert.ok(captured.message.includes("TRIGGER.md"), captured.message);
});

test("install --from: only TRIGGER.md present exits 1 with ERR_SKILL_MISSING", async () => {
  const { sampleDir } = setupSampleDir({ withSkill: false });
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured?.code, "ERR_SKILL_MISSING");
  assert.ok(captured.message.includes("SKILL.md"), captured.message);
});

test("install --from: path is a file, not a directory → ERR_PATH_NOT_FOUND with reason", async () => {
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const filePath = join(root, "not-a-directory.txt");
  writeFileSync(filePath, "hello");
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", filePath],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured?.code, "ERR_PATH_NOT_FOUND");
  assert.ok(captured.message.includes("not a directory"), captured.message);
});

test("install --from: unicode in SKILL.md + TRIGGER.md reaches the POST body byte-for-byte", async () => {
  // Verifies no encoding conversion / text processing happens between disk
  // read and the POST body. CJK + emoji + zero-width joiner are the usual
  // offenders for accidental NFC normalization or UTF-16 re-encoding.
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const sampleDir = join(root, "pkg");
  mkdirSync(sampleDir);
  const skillBody = "---\nname: pkg\n---\n# 中文 👨‍👩‍👧 skill body\n";
  const triggerBody = "---\nname: pkg\ndescription: café ☕ ‍ joiner\n---\n";
  writeFileSync(join(sampleDir, "SKILL.md"), skillBody);
  writeFileSync(join(sampleDir, "TRIGGER.md"), triggerBody);
  let capturedBody = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, _path, opts) => {
        capturedBody = JSON.parse(opts.body);
        return { zombie_id: "zom_01abc", name: "pkg", status: "active" };
      },
    }),
  );
  assert.equal(code, 0);
  assert.equal(capturedBody.source_markdown, skillBody);
  assert.equal(capturedBody.trigger_markdown, triggerBody);
});

test("install --from: POST body contains exactly trigger_markdown + source_markdown, no extras", async () => {
  // Contract test — the server's create handler accepts {trigger_markdown,
  // source_markdown} and parses TRIGGER.md frontmatter to derive name +
  // config_json. A leaked CLI-derived `name` or `config_json` would either
  // be ignored or trip schema validation; either way it's wasted bytes
  // and a divergence point. Fail loud if the shape regresses.
  const { sampleDir } = setupSampleDir();
  let capturedBody = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, _path, opts) => {
        capturedBody = JSON.parse(opts.body);
        return { zombie_id: "zom_01abc", name: "test-zombie", status: "active" };
      },
    }),
  );
  assert.equal(code, 0);
  const keys = Object.keys(capturedBody).sort();
  assert.deepEqual(keys, ["source_markdown", "trigger_markdown"],
    `POST body keys must be exactly [source_markdown, trigger_markdown], got [${keys.join(", ")}]`);
});

// ── §2 — Server response handling ─────────────────────────────────────────

test("install --from: 409 conflict bubbles up an ApiError-shaped error for cli.js to render", async () => {
  const { sampleDir } = setupSampleDir();
  let caught = null;
  try {
    await commandZombie(
      { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => {
          const err = new Error("zombie named 'test-zombie' already exists");
          err.name = "ApiError";
          err.status = 409;
          err.code = "ERR_ZOMBIE_NAME_CONFLICT";
          throw err;
        },
      }),
    );
  } catch (e) {
    caught = e;
  }
  assert.ok(caught, "expected error to bubble up");
  assert.equal(caught.code, "ERR_ZOMBIE_NAME_CONFLICT");
  assert.equal(caught.status, 409);
});

test("install --from: 400 ERR_ZOMBIE_INVALID_CONFIG bubbles up (frontmatter parse failure)", async () => {
  // Server-side parse means broken TRIGGER.md (missing name, bad YAML, etc.)
  // surfaces as ERR_ZOMBIE_INVALID_CONFIG. The CLI just bubbles it; cli.js
  // renders the structured error with request_id.
  const { sampleDir } = setupSampleDir();
  let caught = null;
  try {
    await commandZombie(
      { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => {
          const err = new Error("Config JSON is not valid. Check trigger, tools, and budget fields.");
          err.name = "ApiError";
          err.status = 400;
          err.code = "ERR_ZOMBIE_INVALID_CONFIG";
          throw err;
        },
      }),
    );
  } catch (e) {
    caught = e;
  }
  assert.ok(caught, "ERR_ZOMBIE_INVALID_CONFIG must bubble for cli.js to render");
  assert.equal(caught.code, "ERR_ZOMBIE_INVALID_CONFIG");
  assert.equal(caught.status, 400);
});

test("install --from: non-ApiError network failure surfaces as IO_ERROR exit 1", async () => {
  const { sampleDir } = setupSampleDir();
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("ECONNREFUSED"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured?.code, "IO_ERROR");
  assert.ok(captured.message.includes("ECONNREFUSED"), captured.message);
});

test("install --from: ApiError re-throws for cli.js printApiError to render", async () => {
  const { sampleDir } = setupSampleDir();
  let caught = null;
  try {
    await commandZombie(
      { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => {
          const err = new Error("internal server error");
          err.name = "ApiError";
          err.status = 500;
          err.code = "INTERNAL_SERVER_ERROR";
          throw err;
        },
      }),
    );
  } catch (e) {
    caught = e;
  }
  assert.ok(caught, "ApiError must bubble");
  assert.equal(caught.code, "INTERNAL_SERVER_ERROR");
  assert.equal(caught.status, 500);
});

// ── §3 — Shared pre-flight ────────────────────────────────────────────────

test("install --from: not-authenticated bubbles up the auth error", async () => {
  const { sampleDir } = setupSampleDir();
  let caught = null;
  try {
    await commandZombie(
      { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => {
          const err = new Error("authentication required");
          err.name = "ApiError";
          err.status = 401;
          err.code = "NOT_AUTHENTICATED";
          throw err;
        },
      }),
    );
  } catch (e) {
    caught = e;
  }
  assert.ok(caught, "expected auth error to bubble up");
  assert.equal(caught.code, "NOT_AUTHENTICATED");
  assert.equal(caught.status, 401);
});

test("install --from: no workspace selected exits 1 with NO_WORKSPACE", async () => {
  const { sampleDir } = setupSampleDir();
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    noWorkspace,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured?.code, "NO_WORKSPACE");
});

test("install --from --json: emits JSON with server-returned name", async () => {
  const { sampleDir } = setupSampleDir();
  const stdout = makeStdout();
  let payload = null;
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: true, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      printJson: (_stream, obj) => { payload = obj; },
    }),
  );
  assert.equal(code, 0);
  assert.ok(payload);
  assert.equal(payload.status, "installed");
  assert.equal(payload.zombie_id, "zom_01abc");
  assert.equal(payload.name, "test-zombie");
  const out = stdout.lines.join("");
  assert.ok(!out.includes("🎉"), `no emoji in JSON mode:\n${out}`);
  assert.ok(!out.includes("is live"), `no prose in JSON mode:\n${out}`);
});

// ── §4 — Removed commands ─────────────────────────────────────────────────

test("route: 'up' no longer resolves to a handler", () => {
  assert.equal(findRoute("up", []), null);
});

test("install --from (no value): boolean-true treated as missing argument", async () => {
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install"],
    workspaces,
    {
      parseFlags: () => ({ options: { from: true }, positionals: [] }),
      request: async () => { throw new Error("request should not be called"); },
      apiHeaders: () => ({}),
      ui,
      printJson: () => {},
      printKeyValue: () => {},
      printSection: () => {},
      writeLine: () => {},
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
    },
  );
  assert.equal(code, 2);
  assert.equal(captured?.code, "MISSING_ARGUMENT");
  assert.ok(captured.message.includes("--from"), captured.message);
  assert.ok(!captured.message.includes("true"), `message must not leak boolean: ${captured.message}`);
});

test("install without --from exits 2 with usage pointing at --from", async () => {
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 2);
  assert.equal(captured?.code, "MISSING_ARGUMENT");
  assert.ok(captured.message.includes("--from"), captured.message);
});

test("install <bundled-name>: legacy usage exits 2 and points at --from", async () => {
  let captured = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "platform-ops"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured = { code: errCode, message }; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 2);
  assert.ok(captured, "expected writeError to fire");
  assert.ok(["MISSING_ARGUMENT", "UNKNOWN_ARGUMENT"].includes(captured.code), `got ${captured.code}`);
  assert.ok(captured.message.includes("--from"), captured.message);
});

test("filesystem: bundled templates/ and legacy up test are gone", () => {
  const templatesPath = join(PKG_ROOT, "templates");
  const legacyUpTest = join(TEST_DIR, "zombie-up-woohoo.unit.test.js");
  assert.ok(!existsSync(templatesPath), `should be deleted: ${templatesPath}`);
  assert.ok(!existsSync(legacyUpTest), `should be deleted: ${legacyUpTest}`);
});

// ── §5 — Error-message sweep ──────────────────────────────────────────────

test("status empty-list hint points at `install --from <path>`, not legacy strings", async () => {
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["status"],
    workspaces,
    makeDeps({
      request: async () => ({ items: [] }),
    }),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("zombiectl install --from"), `hint must mention --from:\n${out}`);
  assert.ok(!out.includes("install <template>"), `no legacy <template>:\n${out}`);
  assert.ok(!out.includes("zombiectl up"), `no reference to removed 'zombiectl up':\n${out}`);
});
