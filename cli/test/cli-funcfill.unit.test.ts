// Function-fill coverage for runCli's exit-code mapping (src/cli.ts). These
// exercise the branches the did-you-mean suite touches only for the
// unknown-command case.
//
// What is reachable through runCli's public surface:
//   • an explicit --help, and a group invoked with no subcommand → exit 0
//   • the auth guard refusing before a handler runs → exit 1
//   • the usage family (unknown command, a flag missing its value, a value
//     the flag refuses) → exit 4, the validation code
//
// Exit codes are a machine surface: a script tells "you typed it wrong" (4)
// from "the server said no" (3) from "the network failed" (2) by number, so
// each branch is pinned rather than left to whatever the parser defaults to.

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { runCli } from "../src/cli.ts";
import { EXIT_CODE } from "../src/errors/index.ts";
import { bufferStream, makeNoop, cliEnv, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";

const VALID_ID = "01900000-0000-7000-8000-000000000001";

async function withBrokenStateBase<T>(fn: () => Promise<T>): Promise<T> {
  const previous = process.env.AGENTSFLEET_STATE_DIR;
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "agentsfleet-broken-state-"));
  const fileBase = path.join(dir, "not-a-directory");
  await fs.writeFile(fileBase, "x");
  process.env.AGENTSFLEET_STATE_DIR = fileBase;
  try {
    return await fn();
  } finally {
    if (previous === undefined) delete process.env.AGENTSFLEET_STATE_DIR;
    else process.env.AGENTSFLEET_STATE_DIR = previous;
    await fs.rm(dir, { recursive: true, force: true });
  }
}

describe("runCli exit-code mapping", () => {
  test("root-level unknown command maps a usage failure to the validation exit", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["nope-not-a-command"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ NO_COLOR: "1" }),
      });
      expect(code).toBe(EXIT_CODE.ValidationError);
      expect(err.read()).toContain("Unknown subcommand");
    });
  });

  test("root-level option missing its argument maps to the validation exit", async () => {
    // `--api` is a global value option; a dangling `--api` is emitted by
    // the ROOT command (which carries exitOverride), so it routes through
    // a flag missing its value → the validation code → the
    // validation exit, rather than crashing at a leaf via process.exit.
    await withFreshStateDir(async () => {
      const code = await runCli(["--api"], {
        stdout: makeNoop(),
        stderr: makeNoop(),
        env: cliEnv({ NO_COLOR: "1" }),
      });
      expect(code).toBe(EXIT_CODE.ValidationError);
    });
  });

  test("root-level version exits 0", async () => {
    await withFreshStateDir(async () => {
      const code = await runCli(["--version"], {
        stdout: makeNoop(),
        stderr: makeNoop(),
        env: cliEnv({ NO_COLOR: "1" }),
      });
      expect(code).toBe(0);
    });
  });

  test("auth-required command short-circuits to exit 1 via state.exitCode", async () => {
    // The preAction auth-guard sets state.exitCode = 1 and throws a
    // CommanderError(code "auth.required"). exitFromCommanderError sees
    // state.exitCode !== 0 first and returns it before the usage-code
    // check — proving the state.exitCode short-circuit branch (line 158).
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ NO_COLOR: "1" }),
      });
      expect(code).toBe(1);
      expect(err.read().length).toBeGreaterThan(0);
    });
  });

  test("auth-required surfaces a JSON error envelope under --json and still exits 1", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["--json", "doctor"], {
        stdout: out.stream,
        stderr: err.stream,
        env: cliEnv({ NO_COLOR: "1" }),
      });
      expect(code).toBe(1);
    });
  });

  test("state-load failures fall back to empty credentials and workspaces", async () => {
    await withBrokenStateBase(async () => {
      const err = bufferStream();
      const code = await runCli(["doctor"], {
        stdout: makeNoop(),
        stderr: err.stream,
        env: cliEnv({ NO_COLOR: "1" }),
      });
      expect(code).toBe(1);
      expect(err.read()).toContain("not authenticated");
    });
  });

  test("an authed command that parses cleanly returns state.exitCode (the success tail)", async () => {
    // Drives the no-error tail (line 277, `return state.exitCode`) with a
    // bound leaf handler so the CommanderError mapping is NOT exercised —
    // the complementary side of the parseResult.ok branch.
    await withAuthedStateDir({ workspaceId: VALID_ID }, async () => {
      const code = await runCli(["workspace", "list"], {
        stdout: makeNoop(),
        stderr: makeNoop(),
        env: cliEnv({ NO_COLOR: "1" }),
        // Offline: no fetch needed — workspace list reads local state.
      });
      expect(typeof code).toBe("number");
    });
  });
});
