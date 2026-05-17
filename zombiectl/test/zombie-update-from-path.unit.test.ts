// `zombiectl zombie update <id> --from <path>` — re-parse + PATCH zombie
// from a local skill bundle. Mirrors install's loader/preflight/body shape;
// adds zombie-id positional validation and config_revision rendering. The
// CLI is launched fresh per invocation: no concurrency-token flag — LWW
// with row-lock + field-merge is the contract.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { tmpdir } from "node:os";
import { mkdtempSync, writeFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

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

const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0a6f11";

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

function apiError(message: string, fields: { status: number; code: string }): Error {
  return Object.assign(new Error(message), { name: "ApiError", ...fields });
}

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    request: async () => ({
      zombie_id: ZOMBIE_ID,
      config_revision: 1747900000000,
    }),
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
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

function setupSampleDir({ name = "platform-ops", withSkill = true, withTrigger = true }: SampleDirOpts = {}): { root: string; sampleDir: string } {
  const root = mkdtempSync(join(tmpdir(), "update-from-test-"));
  const sampleDir = join(root, name);
  mkdirSync(sampleDir);
  if (withSkill) {
    writeFileSync(join(sampleDir, "SKILL.md"), `---\nname: ${name}\n---\n# skill body\n`);
  }
  if (withTrigger) {
    writeFileSync(
      join(sampleDir, "TRIGGER.md"),
      `---\nname: ${name}\ntriggers:\n  - type: cron\n    schedule: "*/30 * * * *"\ntools:\n  - agentmail\nbudget:\n  daily_dollars: 1.0\n---\n# trigger body\n`,
    );
  }
  return { root, sampleDir };
}

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

interface Captured {
  code?: string;
  message?: string;
}

interface ApiErrorShape extends Error {
  status?: number;
  code?: string;
}

function asString(body: unknown): string {
  if (typeof body !== "string") throw new Error("expected string body");
  return body;
}

const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [] };
const noWorkspace: Workspaces = { current_workspace_id: null, items: [] };

// ── §1 — Happy path ──────────────────────────────────────────────────────

interface PatchRecord {
  path: string;
  method: string;
  body: { source_markdown?: string; trigger_markdown?: string };
}

interface UpdateJson {
  status: string;
  zombie_id: string;
  config_revision: number;
}

test("zombie update: happy path PATCHes /zombies/{id} and prints config_revision", async () => {
  const { sampleDir } = setupSampleDir();
  const captured: { call: PatchRecord | null } = { call: null };
  const stdout = makeStdout();
  const code = await commandZombie(
    makeCtx({ stdout }),
    ["update", ZOMBIE_ID, "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, path, opts) => {
        captured.call = {
          path,
          method: opts?.method ?? "PATCH",
          body: JSON.parse(asString(opts?.body)) as PatchRecord["body"],
        };
        return { zombie_id: ZOMBIE_ID, config_revision: 1747900000000 };
      },
    }),
  );
  assert.equal(code, 0);
  const call = captured.call;
  if (!call) throw new Error("expected PATCH to fire");
  assert.equal(call.method, "PATCH");
  assert.ok(call.path.endsWith(`/zombies/${ZOMBIE_ID}`), `expected zombie path, got: ${call.path}`);
  const out = stdout.lines.join("");
  assert.ok(out.includes(`${ZOMBIE_ID} updated.`), `expected update line:\n${out}`);
  assert.ok(out.includes("Config revision: 1747900000000"), `expected revision line:\n${out}`);
});

test("zombie update: POST body contains exactly trigger_markdown + source_markdown", async () => {
  const { sampleDir } = setupSampleDir();
  const captured: { body: PatchRecord["body"] | null } = { body: null };
  const code = await commandZombie(
    makeCtx(),
    ["update", ZOMBIE_ID, "--from", sampleDir],
    workspaces,
    makeDeps({
      request: async (_ctx, _path, opts) => {
        captured.body = JSON.parse(asString(opts?.body)) as PatchRecord["body"];
        return { zombie_id: ZOMBIE_ID, config_revision: 1 };
      },
    }),
  );
  assert.equal(code, 0);
  const body = captured.body;
  if (!body) throw new Error("expected PATCH body");
  const keys = Object.keys(body).sort();
  assert.deepEqual(keys, ["source_markdown", "trigger_markdown"],
    `PATCH body keys must be exactly [source_markdown, trigger_markdown], got [${keys.join(", ")}]`);
});

test("zombie update --json: emits { status: 'updated', zombie_id, config_revision }", async () => {
  const { sampleDir } = setupSampleDir();
  const captured: { json: UpdateJson | null } = { json: null };
  const stdout = makeStdout();
  const code = await commandZombie(
    makeCtx({ stdout, jsonMode: true }),
    ["update", ZOMBIE_ID, "--from", sampleDir],
    workspaces,
    makeDeps({
      printJson: (_stream, obj) => { captured.json = obj as UpdateJson; },
    }),
  );
  assert.equal(code, 0);
  assert.deepEqual(captured.json, {
    status: "updated",
    zombie_id: ZOMBIE_ID,
    config_revision: 1747900000000,
  });
  const out = stdout.lines.join("");
  assert.ok(!out.includes("updated."), `no prose in JSON mode:\n${out}`);
});

// ── §2 — Argument validation ─────────────────────────────────────────────

test("zombie update without zombie_id exits 2 with usage", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 2);
  assert.equal(captured.code, "MISSING_ARGUMENT");
  assert.ok(captured.message?.includes("zombie update"), captured.message);
  assert.ok(captured.message?.includes("--from"), captured.message);
});

test("zombie update with zombie_id but no --from exits 2", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update", ZOMBIE_ID],
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

test("zombie update with invalid zombie_id exits 2 with VALIDATION_ERROR", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update", "not-a-uuid", "--from", "/tmp"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 2);
  assert.equal(captured.code, "VALIDATION_ERROR");
});

// ── §3 — Loader errors ───────────────────────────────────────────────────

test("zombie update: missing directory exits 1 with ERR_PATH_NOT_FOUND", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update", ZOMBIE_ID, "--from", "/nonexistent/zombie-update-test-path"],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "ERR_PATH_NOT_FOUND");
});

test("zombie update: only SKILL.md present exits 1 with ERR_TRIGGER_MISSING", async () => {
  const { sampleDir } = setupSampleDir({ withTrigger: false });
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update", ZOMBIE_ID, "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "ERR_TRIGGER_MISSING");
});

// ── §4 — Server response handling ────────────────────────────────────────

test("zombie update: 404 ZOMBIE_NOT_FOUND bubbles up", async () => {
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["update", ZOMBIE_ID, "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("zombie not found", { status: 404, code: "UZ-ZMB-NOT-FOUND" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "404 must bubble for cli.ts to render");
  assert.equal(caught.status, 404);
});

test("zombie update: 503 lock_timeout bubbles up for cli.ts retry-hint rendering", async () => {
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["update", ZOMBIE_ID, "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("database busy", { status: 503, code: "UZ-INTERNAL-001" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "503 must bubble");
  assert.equal(caught.status, 503);
  assert.equal(caught.code, "UZ-INTERNAL-001");
});

test("zombie update: ApiError 409 (FSM transition) bubbles up", async () => {
  const { sampleDir } = setupSampleDir();
  let caught: ApiErrorShape | null = null;
  try {
    await commandZombie(
      makeCtx(),
      ["update", ZOMBIE_ID, "--from", sampleDir],
      workspaces,
      makeDeps({
        request: async () => { throw apiError("zombie is killed", { status: 409, code: "UZ-ZMB-010" }); },
      }),
    );
  } catch (e) {
    if (e instanceof Error) caught = e as ApiErrorShape;
  }
  assert.ok(caught, "409 must bubble");
  assert.equal(caught.status, 409);
});

test("zombie update: non-ApiError network failure surfaces as IO_ERROR exit 1", async () => {
  const { sampleDir } = setupSampleDir();
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update", ZOMBIE_ID, "--from", sampleDir],
    workspaces,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("ECONNREFUSED"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "IO_ERROR");
});

// ── §5 — Pre-flight ──────────────────────────────────────────────────────

test("zombie update: no workspace selected exits 1 with NO_WORKSPACE", async () => {
  const captured: Captured = {};
  const code = await commandZombie(
    makeCtx(),
    ["update", ZOMBIE_ID, "--from", "/tmp"],
    noWorkspace,
    makeDeps({
      writeError: (_ctx, errCode, message) => { captured.code = errCode; captured.message = message; },
      request: async () => { throw new Error("request should not be called"); },
    }),
  );
  assert.equal(code, 1);
  assert.equal(captured.code, "NO_WORKSPACE");
});
