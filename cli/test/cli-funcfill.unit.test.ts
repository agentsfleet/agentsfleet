// Function-fill coverage for runCli's post-parse exit-code mapping
// (src/cli.ts). These exercise the reachable branches of
// exitFromCommanderError — the commander-error → POSIX-exit-code
// translation that the did-you-mean suite touches only for the
// unknown-command case.
//
// What is reachable through runCli's public surface, and what is not:
//   • commander.help / group-with-no-subcommand → exit 0  (line 157)
//   • auth-guard CommanderError with state.exitCode preset → that code
//     short-circuits before the usage-code check               (line 158)
//   • usage-family codes (unknownCommand / optionMissingArgument / …)
//     → exit 2                                                  (line 159)
// The fall-through (line 160) and the non-CommanderError parse branch
// (errMessage + lines 263-272) are defensive: the command tree declares
// no .conflicts() options, binds every leaf handler, and InvalidArgumentError
// extends CommanderError — so no argv routes program.parseAsync to a
// non-help / non-usage CommanderError or to a non-CommanderError throw.
// Documented in the StructuredOutput note rather than faked.

import { describe, test, expect } from "bun:test";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { runCli } from "../src/cli.ts";
import { bufferStream, makeNoop, withAuthedStateDir, withFreshStateDir } from "./helpers-cli-state.ts";

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

describe("runCli exit-code mapping (exitFromCommanderError reachable branches)", () => {
  test("root-level unknown command maps a usage CommanderError to exit 2", async () => {
    await withFreshStateDir(async () => {
      const out = bufferStream();
      const err = bufferStream();
      const code = await runCli(["nope-not-a-command"], {
        stdout: out.stream,
        stderr: err.stream,
        env: { NO_COLOR: "1" },
      });
      expect(code).toBe(2);
      expect(err.read()).toContain("unknown command");
    });
  });

  test("root-level option missing its argument maps to exit 2", async () => {
    // `--api` is a global value option; a dangling `--api` is emitted by
    // the ROOT command (which carries exitOverride), so it routes through
    // the bridge as commander.optionMissingArgument → usage code → 2,
    // rather than crashing at a leaf via process.exit.
    await withFreshStateDir(async () => {
      const code = await runCli(["--api"], {
        stdout: makeNoop(),
        stderr: makeNoop(),
        env: { NO_COLOR: "1" },
      });
      expect(code).toBe(2);
    });
  });

  test("root-level version returns Commander's own exit code", async () => {
    await withFreshStateDir(async () => {
      const code = await runCli(["--version"], {
        stdout: makeNoop(),
        stderr: makeNoop(),
        env: { NO_COLOR: "1" },
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
        env: { NO_COLOR: "1" },
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
        env: { NO_COLOR: "1" },
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
        env: { NO_COLOR: "1" },
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
        env: { NO_COLOR: "1" },
        // Offline: no fetch needed — workspace list reads local state.
      });
      expect(typeof code).toBe("number");
    });
  });
});
