import test from "node:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { runCli } from "../src/cli.js";

function bufferStream() {
  let data = "";
  return {
    stream: new Writable({
      write(chunk, _enc, cb) {
        data += String(chunk);
        cb();
      },
    }),
    read: () => data,
  };
}

test("harness source put uploads markdown file content as source_markdown", async () => {
  const out = bufferStream();
  const err = bufferStream();
  const tmpDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-harness-"));
  const filePath = path.join(tmpDir, "agent-profile.md");
  const markdown = "# Harness\n\n```json\n{\"profile_id\":\"ws_123-harness\",\"stages\":[]}\n```";
  await fs.writeFile(filePath, markdown, "utf8");

  try {
    const fetchImpl = async (url, options) => {
      assert.equal(url, "http://localhost:3000/v1/workspaces/ws_123/harness/source");
      assert.equal(options.method, "PUT");
      const payload = JSON.parse(String(options.body));
      assert.equal(payload.agent_id, "ws_123-harness");
      assert.equal(payload.name, "agent-profile");
      assert.equal(payload.source_markdown, markdown);
      return {
        ok: true,
        status: 200,
        text: async () => JSON.stringify({ config_version_id: "pver_123" }),
      };
    };

    const code = await runCli(
      [
        "harness",
        "source",
        "put",
        "--workspace-id",
        "ws_123",
        "--file",
        filePath,
        "--profile-id",
        "ws_123-harness",
      ],
      {
        env: { ...process.env, ZOMBIE_TOKEN: "header.payload.sig" },
        stdout: out.stream,
        stderr: err.stream,
        fetchImpl,
      },
    );

    assert.equal(code, 0);
    assert.equal(err.read(), "");
    assert.match(out.read(), /config_version_id=pver_123/);
  } finally {
    await fs.rm(tmpDir, { recursive: true, force: true });
  }
});
