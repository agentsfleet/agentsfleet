import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandAgentHarness } from "../src/commands/agent_harness.js";
import {
  AGENT_ID,
  makeBufferStream,
  makeNoop,
  ui,
} from "./helpers.js";

const CHANGE_ID = "0195b4ba-8d3a-7f13-8abc-000000000092";

test("commandAgentHarness revert posts to the revert endpoint", async () => {
  let called = null;
  const deps = {
    request: async (_ctx, url, init) => {
      called = { url, init };
      return {
        change_id: "0195b4ba-8d3a-7f13-8abc-000000000093",
        reverted_from: CHANGE_ID,
      };
    },
    apiHeaders: () => ({ authorization: "Bearer t" }),
    printJson: () => {},
    ui,
    writeLine: () => {},
  };

  const parsed = { options: { "to-change": CHANGE_ID }, positionals: ["revert", AGENT_ID] };
  const code = await commandAgentHarness({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 0);
  assert.match(called.url, /\/harness\/changes\/.*:revert$/);
  assert.equal(called.init.method, "POST");
});

test("commandAgentHarness revert requires --to-change", async () => {
  const stderr = makeBufferStream();
  const deps = {
    request: async () => {
      throw new Error("should not be called");
    },
    apiHeaders: () => ({}),
    printJson: () => {},
    ui,
    writeLine: (stream, line) => stream.write(`${line}\n`),
  };

  const parsed = { options: {}, positionals: ["revert", AGENT_ID] };
  const code = await commandAgentHarness({ stdout: makeNoop(), stderr: stderr.stream, jsonMode: false }, parsed, AGENT_ID, deps);
  assert.equal(code, 2);
  assert.match(stderr.read(), /requires --to-change/);
});
