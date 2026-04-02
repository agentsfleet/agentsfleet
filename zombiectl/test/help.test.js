import { describe, test, expect } from "bun:test";
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
      expect(output).toContain("workspace remove");
      expect(output).toContain("specs sync");
      expect(output).toContain("run status");
      expect(output).toContain("runs list");
      expect(output).toContain("doctor");
      expect(output).not.toContain("workspace upgrade-scale");
    });
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
    expect(output).toContain("workspace upgrade-scale");
    expect(output).toContain("skill-secret put");
    expect(output).toContain("agent scores");
    expect(output).toContain("agent profile");
    expect(output).toContain("agent improvement-report");
    expect(output).toContain("agent proposals <agent-id> veto <proposal-id>");
    expect(output).toContain("agent harness revert");
  });

  test("--help without ZOMBIE_OPERATOR does NOT contain operator commands", async () => {
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
      expect(output).not.toContain("OPERATOR COMMANDS");
      expect(output).not.toContain("harness source put");
      expect(output).not.toContain("workspace upgrade-scale");
    });
  });

  test("--help with operator token contains operator commands without env override", async () => {
    await withIsolatedStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const token = [
        Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url"),
        Buffer.from(JSON.stringify({ sub: "user_123", role: "operator" })).toString("base64url"),
        "sig",
      ].join(".");
      const code = await runCli(["--help"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { ZOMBIE_TOKEN: token, NO_COLOR: "1" },
      });
      expect(code).toBe(0);
      const output = out.read();
      expect(output).toContain("OPERATOR COMMANDS");
      expect(output).toContain("workspace upgrade-scale");
      expect(output).toContain("admin config set scoring_context_max_tokens");
    });
  });

  test("--help with API_KEY contains operator commands without ZOMBIE_API_KEY", async () => {
    await withIsolatedStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["--help"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { API_KEY: "dev-key", NO_COLOR: "1" },
      });
      expect(code).toBe(0);
      const output = out.read();
      expect(output).toContain("OPERATOR COMMANDS");
      expect(output).toContain("workspace upgrade-scale");
    });
  });

  test("--help uses saved login credentials to show operator commands", async () => {
    await withIsolatedStateDir(async (stateDir) => {
      const out = bufferStream();
      const err = bufferStream();
      const token = [
        Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url"),
        Buffer.from(JSON.stringify({ sub: "user_123", metadata: { role: "operator" } })).toString("base64url"),
        "sig",
      ].join(".");

      await fs.writeFile(path.join(stateDir, "credentials.json"), JSON.stringify({
        token,
        saved_at: Date.now(),
        session_id: "sess_test",
        api_url: null,
      }));

      const code = await runCli(["--help"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { NO_COLOR: "1" },
      });
      expect(code).toBe(0);
      const output = out.read();
      expect(output).toContain("OPERATOR COMMANDS");
      expect(output).toContain("workspace upgrade-scale");
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

  test("--help with admin role token shows OPERATOR COMMANDS", async () => {
    await withIsolatedStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const token = [
        Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url"),
        Buffer.from(JSON.stringify({ sub: "user_admin", role: "admin" })).toString("base64url"),
        "sig",
      ].join(".");
      const code = await runCli(["--help"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { ZOMBIE_TOKEN: token, NO_COLOR: "1" },
      });
      expect(code).toBe(0);
      const output = out.read();
      expect(output).toContain("OPERATOR COMMANDS");
      expect(output).toContain("workspace upgrade-scale");
    });
  });

  test("--help with user role token does NOT show OPERATOR COMMANDS", async () => {
    await withIsolatedStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const token = [
        Buffer.from(JSON.stringify({ alg: "none", typ: "JWT" })).toString("base64url"),
        Buffer.from(JSON.stringify({ sub: "user_basic", role: "user" })).toString("base64url"),
        "sig",
      ].join(".");
      const code = await runCli(["--help"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { ZOMBIE_TOKEN: token, NO_COLOR: "1" },
      });
      expect(code).toBe(0);
      const output = out.read();
      expect(output).not.toContain("OPERATOR COMMANDS");
      expect(output).not.toContain("workspace upgrade-scale");
    });
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
    expect(out.read()).toContain("zombiectl v0.3.0");
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
    expect(parsed.version).toBe("0.3.0");
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
    expect(output).toContain("zombiectl v0.3.0");
  });
});
