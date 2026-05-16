// `zombiectl events <id>` pagination — exercises the events history
// command path with mocked `request`. Covers:
//   - Default limit (no flag) sends limit=50.
//   - --cursor=<token> is forwarded as ?cursor=<token>.
//   - Response with next_cursor renders the follow-up hint line.
//   - --json mode emits the raw envelope and skips the human format.
//
// Pure mocks — the durable Postgres path is exercised by
// src/zombie/event_loop_writepath_integration_test.zig and friends.

import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandEvents } from "../src/commands/zombie_events.ts";
import {
  buildParsed,
  makeBufferStream,
  ui,
  WS_ID,
} from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";
import type { WriteStream } from "../src/output/index.ts";

// writeLine/printSection in CommandDeps accept the narrower `WriteStream`
// surface from output/index.ts in addition to NodeJS.WritableStream; the
// alias keeps the override signatures byte-aligned with the contract.
type StreamArg = WriteStream | NodeJS.WritableStream;

const ZOMBIE_ID = "0195b4ba-8d3a-7f13-8abc-2b3e1e0caa91";
const NEXT_CURSOR = "Y3Vyc29yLW9wYXF1ZS10b2tlbi0xMjM0";

interface EventItem {
  event_id: string;
  actor: string;
  status: string;
  created_at: number;
  response_text: string;
}

function buildItems(count: number, prefix = "evt"): EventItem[] {
  const items: EventItem[] = [];
  for (let i = 0; i < count; i += 1) {
    items.push({
      event_id: `${prefix}-${i}`,
      actor: "steer:test-user",
      status: "processed",
      created_at: 1700000000000 + i * 1000,
      // response_text is what `formatRow` previews into the human output;
      // embed the prefix-index there so test assertions can grep for it.
      response_text: `${prefix}-row-${i}`,
    });
  }
  return items;
}

interface CtxBundle {
  ctx: CommandCtx;
  readStdout: () => string;
  readStderr: () => string;
}

function makeCtx(overrides: Partial<CommandCtx> = {}): CtxBundle {
  const out = makeBufferStream();
  const err = makeBufferStream();
  return {
    ctx: {
      stdout: out.stream,
      stderr: err.stream,
      jsonMode: false,
      apiUrl: "https://api.test.invalid",
      token: "test-token",
      env: {},
      ...overrides,
    },
    readStdout: out.read,
    readStderr: err.read,
  };
}

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    request: async () => ({}),
    apiHeaders: () => ({}),
    // zombie_events.ts defensively checks `ui.warn ?? ui.dim`; spread to add
    // the warn slot that production-shaped `ui` requires.
    ui: { ...ui, warn: ui.dim },
    printJson: () => {},
    printSection: () => {},
    writeLine: () => {},
    writeError: () => {},
    ...overrides,
  };
  return base as unknown as CommandDeps;
}

const workspaces: Workspaces = { current_workspace_id: WS_ID, items: [] };

interface EventsEnvelope {
  items: ReadonlyArray<EventItem>;
  next_cursor?: string;
}

test("events: default limit=50 + first page renders 50 rows + next-cursor hint", async () => {
  const captured: { url: string | null } = { url: null };
  const items = buildItems(50);
  const deps = makeDeps({
    request: async (_ctx, url) => {
      captured.url = url;
      return { items, next_cursor: NEXT_CURSOR };
    },
    printSection: (stream: StreamArg, title: string) => stream.write(`= ${title} =\n`),
    writeLine: (stream: StreamArg, line?: string) => {
      if (line !== undefined) stream.write(`${line}\n`);
      else stream.write("\n");
    },
  });
  const { ctx, readStdout } = makeCtx();
  const code = await commandEvents(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);
  assert.equal(code, 0);

  assert.ok(captured.url?.includes(`/v1/workspaces/${WS_ID}/zombies/${ZOMBIE_ID}/events`), `wrong url: ${captured.url}`);
  assert.ok(captured.url?.includes("limit=50"), `expected limit=50: ${captured.url}`);

  const out = readStdout();
  // 50 rows + section header + cursor hint. formatRow surfaces
  // response_text as the row preview, so each unique row prints
  // `evt-row-<i>`.
  const rowMatches = out.match(/evt-row-\d+/g) ?? [];
  assert.equal(rowMatches.length, 50, `expected 50 row previews: ${rowMatches.length}`);
  assert.ok(out.includes(`zombiectl events ${ZOMBIE_ID} --cursor=${NEXT_CURSOR}`), `missing cursor hint: ${out}`);
});

test("events: --cursor=<token> is forwarded as ?cursor=<token> and 10-row tail renders without next hint", async () => {
  const captured: { url: string | null } = { url: null };
  const items = buildItems(10, "tail");
  const deps = makeDeps({
    request: async (_ctx, url) => {
      captured.url = url;
      return { items }; // no next_cursor
    },
    writeLine: (stream: StreamArg, line?: string) => {
      if (line !== undefined) stream.write(`${line}\n`);
      else stream.write("\n");
    },
  });
  const { ctx, readStdout } = makeCtx();
  const code = await commandEvents(ctx, buildParsed([ZOMBIE_ID, `--cursor=${NEXT_CURSOR}`]), workspaces, deps);
  assert.equal(code, 0);

  assert.ok(captured.url?.includes(`cursor=${NEXT_CURSOR}`), `cursor not forwarded: ${captured.url}`);

  const out = readStdout();
  const rowMatches = out.match(/tail-row-\d+/g) ?? [];
  assert.equal(rowMatches.length, 10);
  assert.ok(!out.includes("More: zombiectl events"), `unexpected next-cursor hint: ${out}`);
});

test("events: 60 events split 50 + 10 across two paginated calls", async () => {
  // First call: no cursor → returns 50 + next_cursor.
  // Second call: with cursor → returns 10 + no next_cursor.
  // Asserts the operator-visible flow ends after the second call.
  let calls = 0;
  const firstPage = buildItems(50, "p1");
  const secondPage = buildItems(10, "p2");
  const deps = makeDeps({
    request: async (_ctx, url) => {
      calls += 1;
      if (calls === 1) {
        assert.ok(!url.includes("cursor="), `first call should not carry cursor: ${url}`);
        return { items: firstPage, next_cursor: NEXT_CURSOR };
      }
      assert.ok(url.includes(`cursor=${NEXT_CURSOR}`), `second call missing cursor: ${url}`);
      return { items: secondPage };
    },
  });

  const { ctx: ctx1 } = makeCtx();
  const code1 = await commandEvents(ctx1, buildParsed([ZOMBIE_ID]), workspaces, deps);
  assert.equal(code1, 0);

  const { ctx: ctx2 } = makeCtx();
  const code2 = await commandEvents(ctx2, buildParsed([ZOMBIE_ID, `--cursor=${NEXT_CURSOR}`]), workspaces, deps);
  assert.equal(code2, 0);

  assert.equal(calls, 2);
});

test("events: --json mode emits raw envelope and skips human format", async () => {
  const captured: { json: EventsEnvelope | null } = { json: null };
  const items = buildItems(3);
  const deps = makeDeps({
    request: async () => ({ items, next_cursor: NEXT_CURSOR }),
    printJson: (_stream, payload) => { captured.json = payload as EventsEnvelope; },
    printSection: () => { throw new Error("printSection must not fire in --json"); },
    writeLine: () => { throw new Error("writeLine must not fire in --json"); },
  });
  const { ctx } = makeCtx();
  const code = await commandEvents(ctx, buildParsed([ZOMBIE_ID, "--json"]), workspaces, deps);
  assert.equal(code, 0);
  assert.deepEqual(captured.json?.items.length, 3);
  assert.equal(captured.json?.next_cursor, NEXT_CURSOR);
});

test("events: empty response prints \"No events yet.\" and exits 0", async () => {
  const deps = makeDeps({
    request: async () => ({ items: [] }),
    printSection: () => { throw new Error("printSection must not fire on empty"); },
    writeLine: (stream: StreamArg, line?: string) => { stream.write(`${line ?? ""}\n`); },
  });
  const { ctx, readStdout } = makeCtx();
  const code = await commandEvents(ctx, buildParsed([ZOMBIE_ID]), workspaces, deps);
  assert.equal(code, 0);
  assert.ok(readStdout().includes("No events yet."));
});

test("events: --actor and --since flags forward to API", async () => {
  const captured: { url: string | null } = { url: null };
  const deps = makeDeps({
    request: async (_ctx, url) => {
      captured.url = url;
      return { items: [] };
    },
  });
  const { ctx } = makeCtx();
  const code = await commandEvents(ctx, buildParsed([ZOMBIE_ID, "--actor=steer:*", "--since=2h"]), workspaces, deps);
  assert.equal(code, 0);
  assert.ok(captured.url?.includes("actor=steer"), `actor not forwarded: ${captured.url}`);
  assert.ok(captured.url?.includes("since=2h"), `since not forwarded: ${captured.url}`);
});
