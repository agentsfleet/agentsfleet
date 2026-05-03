import { test } from "bun:test";
import assert from "node:assert/strict";
import { commandAdmin } from "../src/commands/admin.js";
import { makeNoop, ui, WS_ID } from "./helpers.js";

test("admin config add scoring_context_max_tokens posts workspace config update", async () => {
  let called = null;
  const deps = {
    parseFlags: () => ({ options: { "workspace-id": WS_ID }, positionals: ["1024"] }),
    request: async (_ctx, url, opts) => {
      called = { url, opts };
      return { workspace_id: WS_ID, scoring_context_max_tokens: 1024 };
    },
    apiHeaders: () => ({ Authorization: "Bearer token" }),
    ui,
    printJson: () => {},
    writeLine: () => {},
  };

  const code = await commandAdmin({ stdout: makeNoop(), stderr: makeNoop(), jsonMode: false }, ["config", "add", "scoring_context_max_tokens"], null, deps);
  assert.equal(code, 0);
  assert.equal(called.url, `/v1/workspaces/${encodeURIComponent(WS_ID)}/scoring/config`);
  assert.equal(called.opts.method, "POST");
  assert.deepEqual(JSON.parse(called.opts.body), { scoring_context_max_tokens: 1024 });
});
