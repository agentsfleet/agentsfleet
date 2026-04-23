// `zombiectl install --from <path>` — unified install command.
//
// Covers §1–§5 dimensions: happy path, missing dir, missing/partial files,
// name fallback, pre-flight errors, --json mode, removed `up` command,
// legacy bundled-usage rejection, status hint sweep, and post-merge
// filesystem state.

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
    request: async () => ({
      zombie_id: "zom_01abc",
      webhook_url: "https://api.usezombie.com/v1/webhooks/zom_01abc",
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

function setupSampleDir({ name = "test-zombie", withSkill = true, withTrigger = true, triggerHasNameLine = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const sampleDir = join(root, name);
  mkdirSync(sampleDir);
  if (withSkill) {
    writeFileSync(join(sampleDir, "SKILL.md"), `---\nname: ${name}\n---\n# skill body\n`);
  }
  if (withTrigger) {
    const body = triggerHasNameLine
      ? `---\nname: ${name}\n---\n# trigger body\n`
      : `---\n# trigger body\n`;
    writeFileSync(join(sampleDir, "TRIGGER.md"), body);
  }
  return { root, sampleDir };
}

const workspaces = { current_workspace_id: WS_ID, items: [] };
const noWorkspace = { current_workspace_id: null, items: [] };

// ── §1 — CLI flag + loader ────────────────────────────────────────────────

test("install --from: happy path prints 'is live' + zombie ID; no webhook in pretty mode", async () => {
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
  assert.ok(out.includes("🎉 test-zombie is live."), `expected 'is live' line:\n${out}`);
  assert.ok(out.includes("Zombie ID: zom_01abc"), `expected zombie ID line:\n${out}`);
  assert.ok(!/webhook/i.test(out), `pretty mode should omit webhook URL:\n${out}`);
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
  // stat.isDirectory() guards against pointing --from at a single file.
  // Removing the guard would let the loader try to readFileSync("<file>/SKILL.md"),
  // which produces a confusing ENOENT error.
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

test("install --from: TRIGGER.md name with surrounding whitespace is trimmed", async () => {
  // If someone drops the .trim() on the name match, the live-message would
  // be `🎉    my-zombie   is live.` — jarring and breaks copy-paste of the name.
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const sampleDir = join(root, "pkg");
  mkdirSync(sampleDir);
  writeFileSync(join(sampleDir, "SKILL.md"), "---\nname: pkg\n---\n");
  writeFileSync(join(sampleDir, "TRIGGER.md"), "---\nname:    spaced-name   \n---\n");
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("🎉 spaced-name is live."), `expected trimmed name:\n${out}`);
});

test("install --from: TRIGGER.md with multiple name: lines picks the first", async () => {
  // Documents intent — the frontmatter parser is permissive about ordering
  // but there's exactly one canonical name. First-match is what the server
  // uses for display/uniqueness, so the CLI must match.
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const sampleDir = join(root, "pkg");
  mkdirSync(sampleDir);
  writeFileSync(join(sampleDir, "SKILL.md"), "---\nname: pkg\n---\n");
  writeFileSync(
    join(sampleDir, "TRIGGER.md"),
    "---\nname: first\nother: x\nname: second\n---\n",
  );
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("🎉 first is live."), `expected first match:\n${out}`);
  assert.ok(!out.includes("second"), `must not pick second match:\n${out}`);
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
        return { zombie_id: "zom_01abc", webhook_url: "https://x/" };
      },
    }),
  );
  assert.equal(code, 0);
  assert.equal(capturedBody.source_markdown, skillBody);
  assert.equal(capturedBody.trigger_markdown, triggerBody);
});

test("install --from: POST body contains exactly source_markdown + trigger_markdown, no extras", async () => {
  // Contract test — the server's create-zombie handler parses a specific
  // shape. Accidentally adding fields (e.g. a `name` or `config_json` leak)
  // would fail schema validation server-side. Catching it here is cheaper
  // than a round trip.
  const { sampleDir } = setupSampleDir();
  let capturedBody = null;
  const code = await commandZombie(
    { stdout: makeStdout(), stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, _path, opts) => {
        capturedBody = JSON.parse(opts.body);
        return { zombie_id: "zom_01abc", webhook_url: "https://x/" };
      },
    }),
  );
  assert.equal(code, 0);
  const keys = Object.keys(capturedBody).sort();
  assert.deepEqual(keys, ["source_markdown", "trigger_markdown"],
    `POST body keys must be exactly [source_markdown, trigger_markdown], got [${keys.join(", ")}]`);
});

test("install --from: TRIGGER.md without name line falls back to basename", async () => {
  const { sampleDir } = setupSampleDir({ name: "my-zombie", triggerHasNameLine: false });
  const stdout = makeStdout();
  const code = await commandZombie(
    { stdout, stderr: makeNoop(), jsonMode: false, noInput: false },
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps(),
  );
  assert.equal(code, 0);
  const out = stdout.lines.join("");
  assert.ok(out.includes("🎉 my-zombie is live."), `expected basename fallback:\n${out}`);
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
  // Simulate what `request()` in http.js actually throws for a 5xx — an
  // ApiError. commandInstall must re-throw these so the top-level handler
  // emits the structured error with request_id, not a generic IO_ERROR.
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

test("install --from --json: emits JSON, no prose", async () => {
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
  assert.ok(payload.webhook_url);
  assert.equal(payload.name, "test-zombie");
  const out = stdout.lines.join("");
  assert.ok(!out.includes("🎉"), `no emoji in JSON mode:\n${out}`);
  assert.ok(!out.includes("is live"), `no prose in JSON mode:\n${out}`);
});

// ── §4 — Removed commands ─────────────────────────────────────────────────

test("route: 'up' no longer resolves to a handler", () => {
  assert.equal(findRoute("up", []), null);
});

test("install --from (no value): boolean-true from parser treated as missing argument, no ugly error", async () => {
  // Simulates `zombiectl install --from` with no value — minimist-style parsers
  // set parsed.options.from = true. Before the guard, statSync(true) would
  // bubble as `ERR_PATH_NOT_FOUND: ERR_PATH_NOT_FOUND: true`.
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
    ["install", "lead-collector"],
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
  // Paths are resolved from the test file — CWD-independent so this assertion
  // can't silently pass if `bun test` is run from an unexpected directory.
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
