// Covers the uncovered branches in src/commands/zombie.ts surfaced by
// the PR #326 codecov delta (commandStatus print loop + empty-list
// branch, commandSetStatus validation_error branch, commandResume +
// commandKill export wrappers, the full commandDelete handler).

import { test, expect } from "bun:test";
import { Writable } from "node:stream";

import {
  commandStatus,
  commandResume,
  commandKill,
  commandDelete,
  commandStop,
} from "../src/commands/zombie.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";
import type { ApiRequestOptions } from "../src/lib/http.ts";
import { buildParsed } from "./helpers.ts";

const sink = (): Writable => new Writable({ write(_c, _e, cb) { cb(); } });

function bufferedSink(): { stream: Writable; read: () => string } {
  let buf = "";
  return {
    stream: new Writable({ write(chunk, _e, cb) { buf += String(chunk); cb(); } }),
    read: () => buf,
  };
}

const passthroughUi = {
  ok: (s: string) => s,
  err: (s: string) => s,
  info: (s: string) => s,
  dim: (s: string) => s,
  head: (s: string) => s,
};

const WS_ID = "0195b4ba-8d3a-7f13-8abc-000000000010";
const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-000000000020";
const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [] };

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  return {
    request: async () => ({ items: [] }),
    apiHeaders: () => ({}),
    ui: passthroughUi,
    printJson: () => {},
    printKeyValue: () => {},
    printSection: () => {},
    writeLine: () => {},
    writeError: () => {},
    ...overrides,
  } as unknown as CommandDeps;
}

// ── commandStatus ─────────────────────────────────────────────────────────

test("commandStatus empty zombies → prints install hint, exit 0", async () => {
  const stdout = bufferedSink();
  const deps = makeDeps({
    request: async () => ({ items: [] }),
    writeLine: (stream, line) => { stream.write((line ?? "") + "\n"); },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: stdout.stream, stderr: sink() };

  const code = await commandStatus(ctx, buildParsed([]), workspaces, deps);

  expect(code).toBe(0);
  expect(stdout.read()).toContain("No zombies running");
  expect(stdout.read()).toContain("zombiectl install --from <path>");
});

test("commandStatus with zombies → prints each row with budget formatting", async () => {
  const stdout = bufferedSink();
  const printed: Array<{ Name?: string; Status?: string; Events?: string; Budget?: string }> = [];
  const sections: string[] = [];
  const deps = makeDeps({
    request: async () => ({
      items: [
        { name: "alpha", status: "active", events_processed: 7, budget_used_dollars: 1.234 },
        { name: "beta", status: "paused", events_processed: 0, budget_used_dollars: null },
      ],
    }),
    printSection: (_stream, label) => { sections.push(label); },
    printKeyValue: (_stream, row) => { printed.push(row as never); },
    writeLine: () => {},
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: stdout.stream, stderr: sink() };

  const code = await commandStatus(ctx, buildParsed([]), workspaces, deps);

  expect(code).toBe(0);
  expect(sections).toEqual(["Zombies"]);
  expect(printed).toHaveLength(2);
  expect(printed[0]).toMatchObject({ Name: "alpha", Status: "active", Events: "7", Budget: "$1.23" });
  expect(printed[1]).toMatchObject({ Name: "beta", Status: "paused", Events: "0", Budget: "—" });
});

test("commandStatus JSON mode → printJson with raw res", async () => {
  let jsonPayload: unknown;
  const deps = makeDeps({
    request: async () => ({ items: [{ name: "x" }] }),
    printJson: (_stream, value) => { jsonPayload = value; },
  });
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdout: sink(),
    stderr: sink(),
    jsonMode: true,
  };

  const code = await commandStatus(ctx, buildParsed([]), workspaces, deps);

  expect(code).toBe(0);
  expect(jsonPayload).toEqual({ items: [{ name: "x" }] });
});

// ── commandSetStatus validation_error branch (via commandStop/Resume/Kill) ──

test("commandStop with invalid zombie_id → exit 2 + VALIDATION_ERROR", async () => {
  const errors: Array<{ code: string; message: string }> = [];
  const deps = makeDeps({
    writeError: (_ctx, code, message) => { errors.push({ code, message }); },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: sink(), stderr: sink() };

  const code = await commandStop(ctx, buildParsed(["not-a-uuid"]), workspaces, deps);

  expect(code).toBe(2);
  expect(errors).toHaveLength(1);
  expect(errors[0]?.code).toBe("VALIDATION_ERROR");
  expect(errors[0]?.message).toContain("zombie_id");
});

// ── commandResume + commandKill wrappers (each is a thin commandSetStatus call) ──

test("commandResume PATCH → status=active, exit 0", async () => {
  let captured: ApiRequestOptions | undefined;
  const deps = makeDeps({
    request: async (_ctx, _url, opts) => { captured = opts; return { ok: true }; },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: sink(), stderr: sink() };

  const code = await commandResume(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);

  expect(code).toBe(0);
  expect(captured?.method).toBe("PATCH");
  expect(JSON.parse(captured?.body ?? "{}")).toEqual({ status: "active" });
});

test("commandKill PATCH → status=killed, exit 0", async () => {
  let captured: ApiRequestOptions | undefined;
  const deps = makeDeps({
    request: async (_ctx, _url, opts) => { captured = opts; return { ok: true }; },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: sink(), stderr: sink() };

  const code = await commandKill(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);

  expect(code).toBe(0);
  expect(captured?.method).toBe("PATCH");
  expect(JSON.parse(captured?.body ?? "{}")).toEqual({ status: "killed" });
});

// ── commandDelete — all branches ──────────────────────────────────────────

test("commandDelete happy path → DELETE + ok line, exit 0", async () => {
  const stdout = bufferedSink();
  let captured: { url?: string; method?: string | undefined } = {};
  const deps = makeDeps({
    request: async (_ctx, url, opts) => { captured = { url, method: opts?.method }; return null; },
    writeLine: (stream, line) => { stream.write((line ?? "") + "\n"); },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: stdout.stream, stderr: sink() };

  const code = await commandDelete(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);

  expect(code).toBe(0);
  expect(captured.method).toBe("DELETE");
  expect(captured.url).toContain(ZOMBIE_ID);
  expect(stdout.read()).toContain(`${ZOMBIE_ID} deleted.`);
});

test("commandDelete JSON mode → printJson with deleted:true", async () => {
  let payload: unknown;
  const deps = makeDeps({
    request: async () => null,
    printJson: (_stream, value) => { payload = value; },
  });
  const ctx: CommandCtx = {
    apiUrl: "https://example",
    stdout: sink(),
    stderr: sink(),
    jsonMode: true,
  };

  const code = await commandDelete(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);

  expect(code).toBe(0);
  expect(payload).toEqual({ zombie_id: ZOMBIE_ID, deleted: true });
});

test("commandDelete missing zombie_id → exit 2 + MISSING_ARGUMENT", async () => {
  const errors: Array<{ code: string; message: string }> = [];
  const deps = makeDeps({
    writeError: (_ctx, code, message) => { errors.push({ code, message }); },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: sink(), stderr: sink() };

  const code = await commandDelete(ctx, buildParsed([]), workspaces, deps);

  expect(code).toBe(2);
  expect(errors[0]?.code).toBe("MISSING_ARGUMENT");
  expect(errors[0]?.message).toContain("zombiectl delete <zombie_id>");
});

test("commandDelete invalid zombie_id → exit 2 + VALIDATION_ERROR", async () => {
  const errors: Array<{ code: string; message: string }> = [];
  const deps = makeDeps({
    writeError: (_ctx, code, message) => { errors.push({ code, message }); },
  });
  const ctx: CommandCtx = { apiUrl: "https://example", stdout: sink(), stderr: sink() };

  const code = await commandDelete(ctx, buildParsed(["bogus-id"]), workspaces, deps);

  expect(code).toBe(2);
  expect(errors[0]?.code).toBe("VALIDATION_ERROR");
  expect(errors[0]?.message).toContain("zombie_id");
});
