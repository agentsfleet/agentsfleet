import { describe, test, expect } from "bun:test";
import { runCli } from "../src/cli.ts";
import { bufferStream } from "./helpers-cli-state.ts";
import { EXIT_CODE } from "../src/errors/index.ts";

// An unknown command is a rejected invocation, not a transport failure —
// exit 2 belongs to NetworkError alone.
const EXIT_VALIDATION = EXIT_CODE.ValidationError;

describe("did-you-mean integration", () => {
  test("'docto' suggests 'doctor'", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["docto"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(EXIT_VALIDATION);
    const errText = err.read();
    expect(errText).toContain("unknown command");
    expect(errText).toContain("docto");
    expect(errText).toContain("doctor");
  });

  test("'workspac' suggests 'workspace create'", async () => {
    const out = bufferStream();
    const err = bufferStream();
    // runCli sees command="workspace" args=["ad"], which IS a valid route (workspace)
    // but the workspace handler itself handles bad subcommands
    // For did-you-mean, we test a truly unknown top-level command
    const code = await runCli(["workspac"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(EXIT_VALIDATION);
    const errText = err.read();
    expect(errText).toContain("unknown command");
    expect(errText).toContain("workspace");
  });

  test("completely unrelated input points at the command list", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["zzzzzzzzzzzzzzzzz"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(EXIT_VALIDATION);
    const errText = err.read();
    expect(errText).toContain("unknown command");
    expect(errText).toContain("--help");
  });

  test("'logn' suggests 'login'", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["logn"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(EXIT_VALIDATION);
    const errText = err.read();
    expect(errText).toContain("login");
  });
});
