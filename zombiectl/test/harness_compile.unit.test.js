import test from "node:test";
import assert from "node:assert/strict";
import { commandHarnessCompile } from "../src/commands/harness_compile.js";
import { makeNoop, ui, AGENT_ID, PVER_ID } from "./helpers.js";

test("commandHarnessCompile sends profile_version_id", async () => {
  let captured = null;
  const deps = {
    request: async (_ctx, reqPath, options) => { captured = { reqPath, options }; return { compile_job_id: "cjob_1", is_valid: true }; },
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: () => {},
  };
  const parsed = { options: { "profile-version-id": PVER_ID }, positionals: [] };
  const code = await commandHarnessCompile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/compile");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.config_version_id, PVER_ID);
  assert.equal(body.agent_id, null);
});

test("commandHarnessCompile sends profile_id selector", async () => {
  let captured = null;
  const deps = {
    request: async (_ctx, reqPath, options) => { captured = { reqPath, options }; return { compile_job_id: "cjob_2", is_valid: true }; },
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: () => {},
  };
  const parsed = { options: { "profile-id": AGENT_ID }, positionals: [] };
  const code = await commandHarnessCompile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  const body = JSON.parse(captured.options.body);
  assert.equal(body.agent_id, AGENT_ID);
  assert.equal(body.config_version_id, null);
});

test("commandHarnessCompile json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => ({ compile_job_id: "cjob_3", is_valid: false }),
    apiHeaders: () => ({}),
    ui,
    printJson: (_stream, v) => { printed = v; },
    writeLine: () => {},
  };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessCompile({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: true }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(printed.compile_job_id, "cjob_3");
});
