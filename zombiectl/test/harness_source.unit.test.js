import test from "node:test";
import assert from "node:assert/strict";
import { Writable } from "node:stream";
import { commandHarnessSourcePut } from "../src/commands/harness_source.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({ write(chunk, _enc, cb) { data += String(chunk); cb(); } }),
    read: () => data,
  };
}

const ui = { ok: (s) => s, err: (s) => s, info: (s) => s };
const noop = new Writable({ write(_c, _e, cb) { cb(); } });

test("commandHarnessSourcePut builds source_markdown payload", async () => {
  const out = bufferStream();
  const err = bufferStream();
  let captured = null;

  const deps = {
    request: async (_ctx, reqPath, options) => { captured = { reqPath, options }; return { profile_version_id: "pver_1" }; },
    apiHeaders: () => ({}),
    ui,
    printJson: () => {},
    writeLine: (stream, line = "") => stream.write(`${line}\n`),
    readFile: async () => "# Harness\n\n```json\n{}\n```",
    resolvePath: (p) => p,
  };

  const parsed = { options: { file: "profile.md", "profile-id": "agent_1" }, positionals: [] };
  const code = await commandHarnessSourcePut({ stdout: out.stream, stderr: err.stream, jsonMode: false }, parsed, "ws_123", deps);

  assert.equal(code, 0);
  assert.equal(err.read(), "");
  assert.equal(captured.reqPath, "/v1/workspaces/ws_123/harness/source");
  const body = JSON.parse(captured.options.body);
  assert.equal(body.profile_id, "agent_1");
  assert.equal(body.name, "profile");
  assert.match(body.source_markdown, /# Harness/);
});

test("commandHarnessSourcePut returns 2 when --file is missing", async () => {
  const err = bufferStream();
  const deps = { ui, writeLine: (stream, line = "") => stream.write(`${line}\n`) };
  const parsed = { options: {}, positionals: [] };
  const code = await commandHarnessSourcePut({ stdout: noop, stderr: err.stream, jsonMode: false }, parsed, "ws_123", deps);
  assert.equal(code, 2);
  assert.match(err.read(), /--file/);
});

test("commandHarnessSourcePut json mode outputs raw response", async () => {
  let printed = null;
  const deps = {
    request: async () => ({ profile_version_id: "pver_9" }),
    apiHeaders: () => ({}),
    ui,
    printJson: (_stream, v) => { printed = v; },
    writeLine: () => {},
    readFile: async () => "# H",
    resolvePath: (p) => p,
  };
  const parsed = { options: { file: "f.md" }, positionals: [] };
  const code = await commandHarnessSourcePut({ stdout: noop, stderr: noop, jsonMode: true }, parsed, "ws_123", deps);
  assert.equal(code, 0);
  assert.equal(printed.profile_version_id, "pver_9");
});
