/**
 * Targeted coverage for `commandWorkspace`'s validation-error branches.
 *
 * The pre-existing "use with malformed id" test (workspace.unit.test.js)
 * hands the command "not-a-uuid", which passes `validateRequiredId` (the
 * alphanumeric SAFE_ID_RE matches it) and routes through the
 * UNKNOWN_WORKSPACE branch instead of the VALIDATION_ERROR branch. As a
 * result `workspace.js:123` and `workspace.js:199` were uncovered.
 *
 * These tests pass an input that fails both UUID_RE and SAFE_ID_RE
 * (special characters), which is the only shape that hits the
 * `VALIDATION_ERROR` branch in `workspace use` and `workspace delete`.
 */
/**
 * Targeted coverage for `commandWorkspace`'s validation-error branches.
 *
 * The pre-existing "use with malformed id" test (workspace.unit.test.ts)
 * hands the command "not-a-uuid", which passes `validateRequiredId` (the
 * alphanumeric SAFE_ID_RE matches it) and routes through the
 * UNKNOWN_WORKSPACE branch instead of the VALIDATION_ERROR branch. As a
 * result the validation-branch lines in workspace.ts were uncovered.
 *
 * These tests pass an input that fails both UUID_RE and SAFE_ID_RE
 * (special characters), which is the only shape that hits the
 * `VALIDATION_ERROR` branch in `workspace use` and `workspace delete`.
 */
import { describe, expect, test } from "bun:test";
import {
  createCoreHandlers,
  makeBufferStream,
  makeNoop,
  ui,
  WS_ID,
} from "./helpers.ts";
import type {
  CommandCtx,
  CommandDeps,
  Workspaces,
} from "../src/commands/types.ts";

const INVALID_ID = "@@@@";

function makeDeps(overrides: Partial<CommandDeps> = {}): CommandDeps {
  const base = {
    clearCredentials: async () => {},
    createSpinner: () => ({ start() {}, stop() {}, succeed() {}, fail() {} }),
    newIdempotencyKey: () => "idem_test",
    openUrl: async () => false,
    printJson: () => {},
    printKeyValue: () => {},
    printTable: () => {},
    request: async () => ({}),
    saveCredentials: async () => {},
    saveWorkspaces: async () => {},
    ui,
    writeLine: (stream: NodeJS.WritableStream, line = "") => stream.write(`${line}\n`),
    apiHeaders: () => ({}),
    ...overrides,
  };
  return base as unknown as CommandDeps;
}

function makeCtx(stderr: NodeJS.WritableStream): CommandCtx {
  return {
    stdout: makeNoop(),
    stderr,
    jsonMode: false,
    apiUrl: "https://api.test",
    env: {},
  };
}

describe("commandWorkspace validation branches", () => {
  test("use with id failing isValidId reports VALIDATION_ERROR", async () => {
    const err = makeBufferStream();
    const ctx = makeCtx(err.stream);
    const workspaces: Workspaces = {
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
    };
    const core = createCoreHandlers(ctx, workspaces, makeDeps());

    const code = await core.commandWorkspace(["use", INVALID_ID]);
    expect(code).toBe(2);
    const msg = err.read();
    // writeError in non-jsonMode emits only the message body. Assert the
    // validateRequiredId failure phrase so a regression that swaps the
    // branch for UNKNOWN_WORKSPACE (different message) lands as a fail.
    expect(msg).toContain("invalid workspace_id");
  });

  test("delete with id failing isValidId reports VALIDATION_ERROR", async () => {
    const err = makeBufferStream();
    const captured: { ws: Workspaces | null } = { ws: null };
    const ctx = makeCtx(err.stream);
    const workspaces: Workspaces = {
      current_workspace_id: WS_ID,
      items: [{ workspace_id: WS_ID, name: "main", created_at: 0 }],
    };
    const core = createCoreHandlers(
      ctx,
      workspaces,
      makeDeps({ saveWorkspaces: async (w) => { captured.ws = w; } }),
    );

    const code = await core.commandWorkspace(["delete", INVALID_ID]);
    expect(code).toBe(2);
    const msg = err.read();
    expect(msg).toContain("invalid workspace_id");
    // saveWorkspaces must NOT fire when validation rejects the id — a
    // regression where the validation gate gets bypassed would silently
    // mutate state.
    expect(captured.ws).toBeNull();
  });
});
