import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Writable } from "node:stream";
import { runCli, VERSION } from "../src/cli.js";

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

async function withIsolatedStateDir(run) {
  const stateDir = await fs.mkdtemp(path.join(os.tmpdir(), "zombiectl-help-"));
  const previousStateDir = process.env.ZOMBIE_STATE_DIR;
  process.env.ZOMBIE_STATE_DIR = stateDir;
  try {
    return await run(stateDir);
  } finally {
    if (previousStateDir === undefined) delete process.env.ZOMBIE_STATE_DIR;
    else process.env.ZOMBIE_STATE_DIR = previousStateDir;
    await fs.rm(stateDir, { recursive: true, force: true });
  }
}

describe("help output", () => {
  test("--help output contains all user commands", async () => {
    await withIsolatedStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["--help"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { NO_COLOR: "1" },
      });
      expect(code).toBe(0);
      const output = out.read();
      expect(output).toContain("login");
      expect(output).toContain("logout");
      expect(output).toContain("workspace add");
      expect(output).toContain("workspace list");
      expect(output).toContain("workspace delete");
      expect(output).toContain("doctor");
      expect(output).not.toContain("run status");
      expect(output).not.toContain("runs list");
      expect(output).not.toContain("spec init");
      expect(output).not.toContain("specs sync");
      expect(output).not.toContain("workspace upgrade-scale");
    });
  });

  test("--help shows environment variables section", async () => {
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    const output = out.read();
    expect(output).toContain("ZOMBIE_API_URL");
    expect(output).toContain("ZOMBIE_TOKEN");
    expect(output).toContain("ZOMBIE_API_KEY");
    expect(output).toContain("NO_COLOR");
  });

  test("--help shows global flags", async () => {
    const out = bufferStream();
    const err = bufferStream();
    await runCli(["--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    const output = out.read();
    expect(output).toContain("--json");
    expect(output).toContain("--no-input");
    expect(output).toContain("--version");
  });
});

describe("version output", () => {
  test("--version shows banner", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env },
    });
    expect(code).toBe(0);
    expect(out.read()).toContain(`zombiectl v${VERSION}`);
  });

  test("--version --json suppresses banner", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--json", "--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env },
    });
    expect(code).toBe(0);
    const parsed = JSON.parse(out.read());
    expect(parsed.version).toBe(VERSION);
  });

  test("--version with NO_COLOR shows plain text", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--version"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    const output = out.read();
    expect(output).toContain(`zombiectl v${VERSION}`);
  });
});
