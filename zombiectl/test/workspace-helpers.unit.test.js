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
import { describe, expect, test } from "bun:test";
import {
  createCoreHandlers,
  makeBufferStream,
  makeNoop,
  ui,
  WS_ID,
} from "./helpers.ts";
const INVALID_ID = "@@@@";

function makeDeps(overrides = {}) {
  return {
    clearCredentials: async () => {},
    createSpinner: () => ({ start() {}, succeed() {}, fail() {} }),
    newIdempotencyKey: () => "idem_test",
    openUrl: async () => false,
    printJson: () => {},
    printKeyValue: () => {},
    printTable: () => {},
    request: async () => ({}),
    saveCredentials: async () => {},
    saveWorkspaces: async () => {},
    ui,
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
    apiHeaders: () => ({}),
    ...overrides,
  };
}

describe("commandWorkspace validation branches", () => {
  test("use with id failing isValidId reports VALIDATION_ERROR (covers workspace.js:123-124)", async () => {
    const err = makeBufferStream();
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [{ workspace_id: WS_ID }] };
    const core = createCoreHandlers(ctx, workspaces, makeDeps());

    const code = await core.commandWorkspace(["use", INVALID_ID]);
    expect(code).toBe(2);
    const msg = err.read();
    // writeError in non-jsonMode emits only the message body. Assert the
    // validateRequiredId failure phrase so a regression that swaps the
    // branch for UNKNOWN_WORKSPACE (different message) lands as a fail.
    expect(msg).toContain("invalid workspace_id");
  });

  test("delete with id failing isValidId reports VALIDATION_ERROR (covers workspace.js:199-200)", async () => {
    const err = makeBufferStream();
    let saved = null;
    const ctx = { stdout: makeNoop(), stderr: err.stream, jsonMode: false, env: {} };
    const workspaces = { current_workspace_id: WS_ID, items: [{ workspace_id: WS_ID }] };
    const core = createCoreHandlers(
      ctx,
      workspaces,
      makeDeps({ saveWorkspaces: async (w) => { saved = w; } }),
    );

    const code = await core.commandWorkspace(["delete", INVALID_ID]);
    expect(code).toBe(2);
    const msg = err.read();
    expect(msg).toContain("invalid workspace_id");
    // saveWorkspaces must NOT fire when validation rejects the id — a
    // regression where the validation gate gets bypassed would silently
    // mutate state.
    expect(saved).toBeNull();
  });
});
