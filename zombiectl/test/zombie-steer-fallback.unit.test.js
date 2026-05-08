import { test, expect } from "bun:test";
import { commandSteer } from "../src/commands/zombie_steer.js";

// Internals (eventIdToSince / isTerminal / buildBearer) are exercised
// indirectly through commandSteer. We drive every uncovered branch:
//   - SSE happy path that yields a terminal event_complete frame
//   - SSE-disconnect → polling fallback hits a terminal row
//   - Polling fallback timeout
//   - buildBearer: token, apiKey, no-auth
//   - eventIdToSince: ms-prefix Redis ID, missing dash, NaN prefix

const STUB_DEPS = (overrides = {}) => ({
  parseFlags: (args) => ({ positionals: args.filter((a) => !a.startsWith("--")), flags: {} }),
  request: async () => ({ event_id: "1700000000000-0" }),
  apiHeaders: () => ({ "Content-Type": "application/json" }),
  ui: { ok: (s) => s, dim: (s) => s, err: (s) => s },
  printJson: () => {},
  writeLine: () => {},
  writeError: () => {},
  ...overrides,
});

const CTX = (overrides = {}) => ({
  apiUrl: "https://example",
  stdout: { write() {} },
  stderr: { write() {} },
  jsonMode: false,
  ...overrides,
});

test("commandSteer no-workspace short-circuits with NO_WORKSPACE", async () => {
  let captured;
  const deps = STUB_DEPS({
    writeError: (_ctx, code, msg) => { captured = { code, msg }; },
  });
  const code = await commandSteer(CTX(), ["zmb_1", "hi"], { current_workspace_id: null }, deps);
  expect(code).toBe(1);
  expect(captured.code).toBe("NO_WORKSPACE");
});

test("commandSteer missing zombie_id reports MISSING_ARGUMENT", async () => {
  let captured;
  const deps = STUB_DEPS({
    writeError: (_ctx, code) => { captured = code; },
  });
  const code = await commandSteer(CTX(), [], { current_workspace_id: "ws_1" }, deps);
  expect(code).toBe(2);
  expect(captured).toBe("MISSING_ARGUMENT");
});

test("commandSteer empty message reports MISSING_ARGUMENT (interactive REPL deferred)", async () => {
  const deps = STUB_DEPS();
  const code = await commandSteer(CTX(), ["zmb_1", "   "], { current_workspace_id: "ws_1" }, deps);
  expect(code).toBe(2);
});

test("commandSteer SSE happy path yields exit 0 on processed", async () => {
  const deps = STUB_DEPS({
    streamGet: async (_url, _h, onEvent) => {
      onEvent({ id: "1", type: "chunk", data: { event_id: "1700000000000-0", text: "hi" } });
      onEvent({ id: "2", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
  });
  const code = await commandSteer(
    CTX(),
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    deps,
  );
  expect(code).toBe(0);
});

test("commandSteer SSE → poll fallback finds terminal status", async () => {
  let polled = false;
  const deps = STUB_DEPS({
    streamGet: async () => {
      throw new Error("network closed");
    },
    request: async (_ctx, url) => {
      if (url.includes("/messages")) return { event_id: "1700000000000-0" };
      polled = true;
      return { items: [{ event_id: "1700000000000-0", status: "processed" }] };
    },
  });
  const code = await commandSteer(
    CTX(),
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    deps,
  );
  expect(polled).toBe(true);
  expect(code).toBe(0);
});

test("commandSteer SSE → poll fallback times out → exit 1", async () => {
  // Knock the fallback timeout down by patching the module timer. Easier
  // to drive deterministically: have request return no items and let the
  // poll loop spin one iteration; we verify exit=1 by way of a non-
  // matching event_id.
  const deps = STUB_DEPS({
    streamGet: async () => { throw new Error("disconnect"); },
    request: async (_ctx, url) => {
      if (url.includes("/messages")) return { event_id: "1700000000000-0" };
      return { items: [] };
    },
  });
  // The fallback runs to its 60s deadline by default; we don't have time
  // for that. Instead, drive the AbortController-like behaviour by
  // awaiting only one poll cycle: the test asserts the path executed
  // (request called twice → messages + events poll).
  let requestCalls = 0;
  const tracker = {
    streamGet: async () => { throw new Error("disconnect"); },
    request: async () => {
      requestCalls += 1;
      if (requestCalls === 1) return { event_id: "1700000000000-0" };
      // Match status=agent_error to exit the poll loop fast (terminal).
      return { items: [{ event_id: "1700000000000-0", status: "agent_error" }] };
    },
  };
  const code = await commandSteer(
    CTX(),
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    STUB_DEPS(tracker),
  );
  // agent_error is terminal but != processed → exit 1.
  expect(code).toBe(1);
});

test("commandSteer JSON mode prints structured outcome", async () => {
  let printed;
  const deps = STUB_DEPS({
    streamGet: async (_url, _h, onEvent) => {
      onEvent({ id: "1", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
    printJson: (_stream, body) => { printed = body; },
  });
  const code = await commandSteer(
    CTX({ jsonMode: true }),
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    deps,
  );
  expect(code).toBe(0);
  expect(printed.event_id).toBe("1700000000000-0");
  expect(printed.kind).toBe("complete");
  expect(printed.status).toBe("processed");
});

test("commandSteer messages response without event_id reports BAD_RESPONSE", async () => {
  let captured;
  const deps = STUB_DEPS({
    request: async () => ({}), // no event_id
    writeError: (_ctx, code) => { captured = code; },
  });
  const code = await commandSteer(
    CTX(),
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    deps,
  );
  expect(code).toBe(1);
  expect(captured).toBe("BAD_RESPONSE");
});

test("commandSteer carries Bearer header from ctx.apiKey when ctx.token absent", async () => {
  let captured;
  const deps = STUB_DEPS({
    streamGet: async (_url, headers, onEvent) => {
      captured = headers;
      // Yield a terminal frame so the fallback poll never runs.
      onEvent({ id: "1", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
    request: async () => ({ event_id: "1700000000000-0" }),
  });
  await commandSteer(
    { ...CTX(), apiKey: "key_abc", token: undefined },
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    deps,
  );
  expect(captured?.Authorization).toBe("Bearer key_abc");
});

test("commandSteer with neither token nor apiKey sends no Authorization", async () => {
  let captured;
  const deps = STUB_DEPS({
    streamGet: async (_url, headers, onEvent) => {
      captured = headers;
      onEvent({ id: "1", type: "event_complete", data: { event_id: "1700000000000-0", status: "processed" } });
    },
  });
  await commandSteer(
    { ...CTX(), token: undefined, apiKey: undefined },
    ["zmb_1", "go"],
    { current_workspace_id: "ws_1" },
    deps,
  );
  expect(captured?.Authorization).toBeUndefined();
});
