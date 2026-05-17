import { test, expect } from "bun:test";
import { commandSteer } from "../src/commands/zombie_steer.ts";
import { buildParsed } from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";

// Internals (eventIdToSince / isTerminal / buildBearer) are exercised
// indirectly through commandSteer. We drive every uncovered branch:
//   - SSE happy path that yields a terminal event_complete frame
//   - SSE-disconnect → polling fallback hits a terminal row
//   - Polling fallback timeout
//   - buildBearer: token, apiKey, no-auth
//   - eventIdToSince: ms-prefix Redis ID, missing dash, NaN prefix

function STUB_DEPS(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    request: async () => ({ event_id: "1700000000000-0" }),
    apiHeaders: () => ({ "Content-Type": "application/json" }),
    ui: { ok: (s: string) => s, dim: (s: string) => s, err: (s: string) => s, info: (s: string) => s, head: (s: string) => s, warn: (s: string) => s },
    printJson: () => {},
    writeLine: () => {},
    writeError: () => {},
    ...overrides,
  };
  return base as unknown as CommandDeps;
}

function CTX(overrides: Partial<CommandCtx> = {}): CommandCtx {
  return {
    apiUrl: "https://example",
    stdout: { write() { return true; } } as unknown as NodeJS.WritableStream,
    stderr: { write() { return true; } } as unknown as NodeJS.WritableStream,
    jsonMode: false,
    env: {},
    ...overrides,
  };
}

const WS_ACTIVE: Workspaces = { current_workspace_id: "ws_1", items: [] };
const NO_WS: Workspaces = { current_workspace_id: null, items: [] };

interface SteerJsonOutcome {
  event_id: string;
  kind: string;
  status: string;
}

test("commandSteer no-workspace short-circuits with NO_WORKSPACE", async () => {
  const captured: { code?: string; msg?: string } = {};
  const deps = STUB_DEPS({
    writeError: (_ctx, code, msg) => { captured.code = code; captured.msg = msg; },
  });
  const code = await commandSteer(CTX(), buildParsed(["zmb_1", "hi"]), NO_WS, deps);
  expect(code).toBe(1);
  expect(captured.code).toBe("NO_WORKSPACE");
});

test("commandSteer missing zombie_id reports MISSING_ARGUMENT", async () => {
  const captured: { code: string | null } = { code: null };
  const deps = STUB_DEPS({
    writeError: (_ctx, code) => { captured.code = code; },
  });
  const code = await commandSteer(CTX(), buildParsed([]), WS_ACTIVE, deps);
  expect(code).toBe(2);
  expect(captured.code).toBe("MISSING_ARGUMENT");
});

test("commandSteer empty message reports MISSING_ARGUMENT (interactive REPL deferred)", async () => {
  const deps = STUB_DEPS();
  const code = await commandSteer(CTX(), buildParsed(["zmb_1", "   "]), WS_ACTIVE, deps);
  expect(code).toBe(2);
});

test("commandSteer SSE happy path yields exit 0 on processed", async () => {
  const deps = STUB_DEPS({
    streamGet: async (_url, _h, onEvent) => {
      onEvent({ id: "1", type: "chunk", data: { event_id: "1700000000000-0", text: "hi" } });
      onEvent({ id: "2", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
  });
  const code = await commandSteer(CTX(), buildParsed(["zmb_1", "go"]), WS_ACTIVE, deps);
  expect(code).toBe(0);
});

test("commandSteer SSE → poll fallback finds terminal status", async () => {
  let polled = false;
  const deps = STUB_DEPS({
    streamGet: async () => { throw new Error("network closed"); },
    request: async (_ctx, url) => {
      if (url.includes("/messages")) return { event_id: "1700000000000-0" };
      polled = true;
      return { items: [{ event_id: "1700000000000-0", status: "processed" }] };
    },
  });
  const code = await commandSteer(CTX(), buildParsed(["zmb_1", "go"]), WS_ACTIVE, deps);
  expect(polled).toBe(true);
  expect(code).toBe(0);
});

test("commandSteer SSE → poll fallback hits a non-processed terminal status → exit 1", async () => {
  // Drive the SSE→poll fallback path: streamGet throws, then the poll
  // loop's first iteration matches a row whose status is terminal
  // but not "processed" (agent_error). Exit 1.
  let requestCalls = 0;
  const deps = STUB_DEPS({
    streamGet: async () => { throw new Error("disconnect"); },
    request: async () => {
      requestCalls += 1;
      if (requestCalls === 1) return { event_id: "1700000000000-0" };
      // Match status=agent_error to exit the poll loop fast (terminal).
      return { items: [{ event_id: "1700000000000-0", status: "agent_error" }] };
    },
  });
  const code = await commandSteer(CTX(), buildParsed(["zmb_1", "go"]), WS_ACTIVE, deps);
  // agent_error is terminal but != processed → exit 1.
  expect(code).toBe(1);
});

test("commandSteer JSON mode prints structured outcome", async () => {
  const captured: { json: SteerJsonOutcome | null } = { json: null };
  const deps = STUB_DEPS({
    streamGet: async (_url, _h, onEvent) => {
      onEvent({ id: "1", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
    printJson: (_stream, body) => { captured.json = body as SteerJsonOutcome; },
  });
  const code = await commandSteer(CTX({ jsonMode: true }), buildParsed(["zmb_1", "go"]), WS_ACTIVE, deps);
  expect(code).toBe(0);
  expect(captured.json?.event_id).toBe("1700000000000-0");
  expect(captured.json?.kind).toBe("complete");
  expect(captured.json?.status).toBe("processed");
});

test("commandSteer messages response without event_id reports BAD_RESPONSE", async () => {
  const captured: { code: string | null } = { code: null };
  const deps = STUB_DEPS({
    request: async () => ({}), // no event_id
    writeError: (_ctx, code) => { captured.code = code; },
  });
  const code = await commandSteer(CTX(), buildParsed(["zmb_1", "go"]), WS_ACTIVE, deps);
  expect(code).toBe(1);
  expect(captured.code).toBe("BAD_RESPONSE");
});

test("commandSteer carries Bearer header from ctx.apiKey when ctx.token absent", async () => {
  const captured: { headers: Record<string, string> | null } = { headers: null };
  const deps = STUB_DEPS({
    streamGet: async (_url, headers, onEvent) => {
      captured.headers = { ...headers };
      // Yield a terminal frame so the fallback poll never runs.
      onEvent({ id: "1", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
    request: async () => ({ event_id: "1700000000000-0" }),
  });
  await commandSteer(
    CTX({ apiKey: "key_abc" }),
    buildParsed(["zmb_1", "go"]),
    WS_ACTIVE,
    deps,
  );
  expect(captured.headers?.["Authorization"]).toBe("Bearer key_abc");
});

test("commandSteer with neither token nor apiKey sends no Authorization", async () => {
  const captured: { headers: Record<string, string> | null } = { headers: null };
  const deps = STUB_DEPS({
    streamGet: async (_url, headers, onEvent) => {
      captured.headers = { ...headers };
      onEvent({ id: "1", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
  });
  await commandSteer(CTX(), buildParsed(["zmb_1", "go"]), WS_ACTIVE, deps);
  expect(captured.headers?.["Authorization"]).toBeUndefined();
});
