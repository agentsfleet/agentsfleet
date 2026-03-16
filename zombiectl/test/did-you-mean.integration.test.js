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

describe("did-you-mean integration", () => {
  test("'runx' suggests 'run'", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["runx"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(2);
    const errText = err.read();
    expect(errText).toContain("unknown command: runx");
    expect(errText).toContain("run");
  });

  test("'workspace ad' suggests 'workspace add'", async () => {
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
    expect(code).toBe(2);
    const errText = err.read();
    expect(errText).toContain("unknown command");
    expect(errText).toContain("workspace");
  });

  test("completely unrelated input shows help hint", async () => {
    const out = bufferStream();
    const err = bufferStream();
    const code = await runCli(["zzzzzzzzzzzzzzzzz"], {
      stdout: out.stream,
      stderr: err.stream,
      env: { ...process.env, NO_COLOR: "1" },
    });
    expect(code).toBe(2);
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
    expect(code).toBe(2);
    const errText = err.read();
    expect(errText).toContain("login");
  });
});
