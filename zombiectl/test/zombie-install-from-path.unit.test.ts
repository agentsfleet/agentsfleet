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

import {
  commandZombieDispatch as commandZombie,
  makeNoop,
  ui,
  WS_ID,
} from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";

const TEST_DIR = dirname(fileURLToPath(import.meta.url));
const PKG_ROOT = dirname(TEST_DIR);

interface FakeStdout {
  write: (s: string) => number;
  lines: string[];
}

function makeStdout(): FakeStdout {
  const lines: string[] = [];
  return {
    write: (s) => { lines.push(s); return lines.length; },
    lines,
  };
}

// Tagged-error shape — matches the duck-typed read path in cli.ts's
// printApiError (status/code/message). Object.assign preserves the
// Error.prototype so `instanceof Error` checks still pass.
function apiError(message: string, fields: { status: number; code: string }): Error {
  return Object.assign(new Error(message), { name: "ApiError", ...fields });
}

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
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
    writeLine: (stream: NodeJS.WritableStream, line = "") => stream.write(line + "\n"),
    writeError: () => {},
    ...overrides,
  };
  return base as unknown as CommandDeps;
}

interface SampleDirOpts {
  name?: string;
  withSkill?: boolean;
  withTrigger?: boolean;
}

function setupSampleDir({ name = "test-zombie", withSkill = true, withTrigger = true }: SampleDirOpts = {}): { root: string; sampleDir: string } {
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

// commandZombieDispatch wants a CommandCtx; the install path passes
// `stdout` to its writeLine helper, so the FakeStdout sat-isfies the
// WritableStream surface the dispatcher actually touches.
function makeCtx(over: { stdout?: FakeStdout | NodeJS.WritableStream; jsonMode?: boolean } = {}): CommandCtx {
  return {
    stdout: (over.stdout ?? makeStdout()) as unknown as NodeJS.WritableStream,
    stderr: makeNoop(),
    jsonMode: over.jsonMode ?? false,
    noInput: false,
    apiUrl: "https://api.test",
    env: {},
  };
}

const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [] };
const noWorkspace: Workspaces = { current_workspace_id: null, items: [] };

// writeError captured-shape — three params, last is optional opts bag we
// ignore in tests.
interface Captured {
  code?: string;
  message?: string;
}

// ── §1 — Loader + happy path ──────────────────────────────────────────────

interface InstallJson {
  status: string;
  zombie_id: string;
  name: string;
}

test("install --from: happy path prints 'is live' from server-returned name", async () => {
  const { sampleDir } = setupSampleDir();
  const stdout = makeStdout();
  const code = await commandZombie(
    makeCtx({ stdout }),
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
    makeCtx({ stdout }),
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
    makeCtx({ stdout }),
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
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", "/nonexistent/install-from-test-path"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "ERR_PATH_NOT_FOUND");
});

test("install --from: only SKILL.md present exits 1 with ERR_TRIGGER_MISSING", async () => {
  const { sampleDir } = setupSampleDir({ withTrigger: false });
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "ERR_TRIGGER_MISSING");
  assert.ok(captured.message?.includes("TRIGGER.md"), captured.message);
});

test("install --from: only TRIGGER.md present exits 1 with ERR_SKILL_MISSING", async () => {
  const { sampleDir } = setupSampleDir({ withSkill: false });
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "ERR_SKILL_MISSING");
  assert.ok(captured.message?.includes("SKILL.md"), captured.message);
});

test("install --from: path is a file, not a directory → ERR_PATH_NOT_FOUND with reason", async () => {
  const root = mkdtempSync(join(tmpdir(), "install-from-test-"));
  const filePath = join(root, "not-a-directory.txt");
  writeFileSync(filePath, "hello");
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", filePath],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "ERR_PATH_NOT_FOUND");
  assert.ok(captured.message?.includes("not a directory"), captured.message);
});

interface InstallBody {
  source_markdown: string;
  trigger_markdown: string;
}

function asString(body: unknown): string {
  if (typeof body !== "string") throw new Error("expected string body");
  return body;
}

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
  const captured: { body: InstallBody | null } = { body: null };
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, _path, opts) => {
        captured.body = JSON.parse(asString(opts?.body)) as InstallBody;
        return { zombie_id: "zom_01abc", name: "pkg", status: "active" };
      },
    }),
  );
  assert.equal(code, 0);
  assert.equal(captured.body?.source_markdown, skillBody);
  assert.equal(captured.body?.trigger_markdown, triggerBody);
});

test("install --from: POST body contains exactly trigger_markdown + source_markdown, no extras", async () => {
  // Contract test — the server's create handler accepts {trigger_markdown,
  // source_markdown} and parses TRIGGER.md frontmatter to derive name +
  // config_json. A leaked CLI-derived `name` or `config_json` would either
  // be ignored or trip schema validation; either way it's wasted bytes
  // and a divergence point. Fail loud if the shape regresses.
  const { sampleDir } = setupSampleDir();
  const captured: { body: InstallBody | null } = { body: null };
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, _path, opts) => {
        captured.body = JSON.parse(asString(opts?.body)) as InstallBody;
        return { zombie_id: "zom_01abc", name: "test-zombie", status: "active" };
      },
    }),
  );
  assert.equal(code, 0);
  const body = captured.body;
  if (!body) throw new Error("expected POST body");
  const keys = Object.keys(body).sort();
  assert.deepEqual(keys, ["source_markdown", "trigger_markdown"],
    `POST body keys must be exactly [source_markdown, trigger_markdown], got [${keys.join(", ")}]`);
});

// ── §2 — Server response handling ─────────────────────────────────────────

interface ApiErrorShape extends Error {
  status?: number;
  code?: string;
}

test("install --from: 409 conflict bubbles up an ApiError-shaped error for cli.ts to render", async () => {
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("zombie named 'test-zombie' already exists", { status: 409, code: "ERR_ZOMBIE_NAME_CONFLICT" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "expected error to bubble up");
  assert.equal(caught.code, "ERR_ZOMBIE_NAME_CONFLICT");
  assert.equal(caught.status, 409);
});

test("install --from: 400 ERR_ZOMBIE_INVALID_CONFIG bubbles up (frontmatter parse failure)", async () => {
  // Server-side parse means broken TRIGGER.md (missing name, bad YAML, etc.)
  // surfaces as ERR_ZOMBIE_INVALID_CONFIG. The CLI just bubbles it; cli.ts
  // renders the structured error with request_id.
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("Config JSON is not valid. Check trigger, tools, and budget fields.", { status: 400, code: "ERR_ZOMBIE_INVALID_CONFIG" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "ERR_ZOMBIE_INVALID_CONFIG must bubble for cli.ts to render");
  assert.equal(caught.code, "ERR_ZOMBIE_INVALID_CONFIG");
  assert.equal(caught.status, 400);
});

test("install --from: non-ApiError network failure surfaces as IO_ERROR exit 1", async () => {
  const { sampleDir } = setupSampleDir();
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("ECONNREFUSED"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "IO_ERROR");
  assert.ok(captured.message?.includes("ECONNREFUSED"), captured.message);
});

test("install --from: ApiError re-throws for cli.ts printApiError to render", async () => {
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("internal server error", { status: 500, code: "INTERNAL_SERVER_ERROR" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "ApiError must bubble");
  assert.equal(caught.code, "INTERNAL_SERVER_ERROR");
  assert.equal(caught.status, 500);
});

// ── §3 — Shared pre-flight ────────────────────────────────────────────────

test("install --from: not-authenticated bubbles up the auth error", async () => {
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["install", "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("authentication required", { status: 401, code: "NOT_AUTHENTICATED" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "expected auth error to bubble up");
  assert.equal(caught.code, "NOT_AUTHENTICATED");
  assert.equal(caught.status, 401);
});

test("install --from: no workspace selected exits 1 with NO_WORKSPACE", async () => {
  const { sampleDir } = setupSampleDir();
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "--from", sampleDir],
    noWorkspace,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "NO_WORKSPACE");
});

test("install --from --json: emits JSON with server-returned name", async () => {
  const { sampleDir } = setupSampleDir();
  const stdout = makeStdout();
  const captured: { json: InstallJson | null } = { json: null };
  const code = await commandZombie(
    makeCtx({ stdout, jsonMode: true }),
    ["install", "--from", sampleDir],
    workspaces,
    makeDeps({
      printJson: (_stream, obj) => { captured.json = obj as InstallJson; },
    }),
  );
  assert.equal(code, 0);
  const payload = captured.json;
  if (!payload) throw new Error("expected JSON payload");
  assert.equal(payload.status, "installed");
  assert.equal(payload.zombie_id, "zom_01abc");
  assert.equal(payload.name, "test-zombie");
  const out = stdout.lines.join("");
  assert.ok(!out.includes("🎉"), `no emoji in JSON mode:\n${out}`);
  assert.ok(!out.includes("is live"), `no prose in JSON mode:\n${out}`);
});

test("install without --from exits 2 with usage pointing at --from", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 2);
  assert.equal(captured.code, "MISSING_ARGUMENT");
  assert.ok(captured.message?.includes("--from"), captured.message);
});

test("install <bundled-name>: legacy usage exits 2 and points at --from", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["install", "platform-ops"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 2);
  assert.ok(captured.code, "expected writeError to fire");
  assert.ok(["MISSING_ARGUMENT", "UNKNOWN_ARGUMENT"].includes(captured.code), `got ${captured.code}`);
  assert.ok(captured.message?.includes("--from"), captured.message);
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
    makeCtx({ stdout }),
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
