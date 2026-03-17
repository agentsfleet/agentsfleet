import { describe, test, expect } from "bun:test";
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

describe("help output", () => {
  test("--help output contains all user commands", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    const output = out.read();
    expect(output).toContain("login");
    expect(output).toContain("logout");
    expect(output).toContain("workspace add");
    expect(output).toContain("workspace list");
    expect(output).toContain("workspace remove");
    expect(output).toContain("specs sync");
    expect(output).toContain("run status");
    expect(output).toContain("runs list");
    expect(output).toContain("doctor");
  });

  test("--help with ZOMBIE_OPERATOR=1 contains operator commands", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, ZOMBIE_OPERATOR: "1", NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    const output = out.read();
    expect(output).toContain("OPERATOR COMMANDS");
    expect(output).toContain("harness source put");
    expect(output).toContain("harness compile");
    expect(output).toContain("harness activate");
    expect(output).toContain("skill-secret put");
    expect(output).toContain("agent scores");
    expect(output).toContain("agent profile");
    expect(output).toContain("agent improvement-report");
    expect(output).toContain("agent proposals <agent-id> veto <proposal-id>");
    expect(output).toContain("agent harness revert");
  });

  test("--help without ZOMBIE_OPERATOR does NOT contain operator commands", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["--help"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, ZOMBIE_OPERATOR: undefined, NO_COLOR: "1" },
    });
    expect(code).toBe(0);
    const output = out.read();
    expect(output).not.toContain("OPERATOR COMMANDS");
    expect(output).not.toContain("harness source put");
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
    expect(out.read()).toContain("zombiectl v0.1.0");
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
    expect(parsed.version).toBe("0.1.0");
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
    expect(output).toContain("zombiectl v0.1.0");
  });
});
