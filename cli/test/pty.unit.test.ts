/**
 * Unit tests for the PtyProcess harness itself
 * (test/acceptance/fixtures/pty.ts).
 *
 * The acceptance login spec leans entirely on this harness to drive the
 * device flow under a real pseudo-terminal, so a regression here silently
 * rots the only end-to-end login coverage we have. These tests run in the
 * unit suite (no live API, no DEV-tenant writes): they spawn the built CLI
 * with a fully-composed hermetic env and exercise nothing but commands whose
 * output and exit code are fixed by `cli.ts` / `cli-tree.ts` — `--version`
 * (prints `agentsfleet v<semver>`, exits 0), `--help` (prints `Usage:`, exits
 * 0), and an unknown command (commander prints `unknown command`, exits 2).
 *
 * No isLive gate: this never touches the network. composeEnv forwards only
 * PATH/HOME plus telemetry-off, so the spawned child neither reads a parent
 * AGENTSFLEET_TOKEN nor flushes PostHog; `--version` returns before any disk
 * read, and `--help`/unknown only read credentials (swallowed) — nothing is
 * written, so no teardown is required and there is no secret to leak.
 *
 * What the harness must guarantee, asserted below:
 *   - waitForLine resolves once a matching line lands (mid-stream AND when
 *     already fully buffered before the call).
 *   - waitForLine rejects on a never-matching predicate after its timeout,
 *     surfacing a `timed out` message — never hangs the suite.
 *   - output strips the carriage returns the pty line discipline echoes.
 *   - exited resolves to the child's REAL exit code (0 for --version, the
 *     non-zero commander code for an unknown command).
 *   - kill resolves exited rather than leaving a zombie.
 *
 * test/** is coverage-excluded (bunfig coveragePathIgnorePatterns), so this
 * guards the harness without touching the 100% src gate.
 */

import { describe, expect, test } from "bun:test";

import { composeEnv } from "./acceptance/fixtures/cli.js";
import { PtyProcess } from "./acceptance/fixtures/pty.ts";

// Worktree mode runs `node dist/bin/agentsfleet.js` — the same artifact the
// acceptance suite drives. The unit suite's `npm run build` keeps dist/ fresh.
const BINARY_MODE = "worktree" as const;

// argv whose stdout/stderr + exit code are pinned by cli.ts / cli-tree.ts.
const VERSION_ARGS = ["--version"] as const;
const HELP_ARGS = ["--help"] as const;
const UNKNOWN_ARGS = ["definitely-not-a-real-command"] as const;

// Pinned exit codes: commander exits 0 on --version/--help; an unrecognised
// command maps through COMMANDER_USAGE_CODES to POSIX usage-error exit 2.
const EXIT_OK = 0;
const EXIT_UNKNOWN_COMMAND = 2;

// Output shapes (substrings/predicates), each asserted from the real CLI.
// The version line is plain under NO_COLOR=1 (no leading status dot).
const VERSION_LINE = /agentsfleet v\d+\.\d+\.\d+/;
const VERSION_NAME = "agentsfleet";
const HELP_USAGE_PREFIX = "Usage:";
const CARRIAGE_RETURN = "\r";

// A predicate that never matches — drives the timeout-rejection path.
const NEVER_MATCH = (): boolean => false;

// Internal waitForLine ceilings. The happy-path ceiling is generous so a
// slow CI box never trips it; the reject ceiling is short so a regression
// (a hang) fails fast instead of stalling the run.
const RESOLVE_WAIT_MS = 8_000;
const REJECT_WAIT_MS = 400;

// bun:test per-test deadlines. These MUST exceed the internal waitForLine
// ceiling above — bun's default 5s deadline would otherwise kill a spawning
// test before waitForLine's own timer can fire, masking the harness's real
// behaviour with an opaque runner timeout.
const SPAWN_TEST_TIMEOUT_MS = 20_000;
const REJECT_TEST_TIMEOUT_MS = 10_000;

const TIMED_OUT_MARKER = "timed out";

/**
 * Spawn the CLI under a pty with a hermetic env. composeEnv forwards only
 * PATH/HOME plus the telemetry-off default, so no parent AGENTSFLEET_TOKEN
 * leaks in and the child never reaches the network. NO_COLOR pins the plain
 * version line so VERSION_LINE has no ANSI/dot prefix to contend with.
 */
function spawnPty(args: ReadonlyArray<string>): PtyProcess {
  const env = composeEnv({ NO_COLOR: "1" });
  return PtyProcess.spawnAgentctl(args, { env, binary: BINARY_MODE });
}

describe("PtyProcess — waitForLine resolution", () => {
  test("resolves once a matching line arrives mid-stream", async () => {
    const pty = spawnPty(VERSION_ARGS);
    const output = await pty.waitForLine((line) => VERSION_LINE.test(line), RESOLVE_WAIT_MS);
    expect(VERSION_LINE.test(output)).toBe(true);
    await pty.exited;
  }, SPAWN_TEST_TIMEOUT_MS);

  test("resolves synchronously when the line is already buffered", async () => {
    const pty = spawnPty(VERSION_ARGS);
    // First await guarantees the version line has been pumped into the
    // buffer; awaiting `exited` alone is NOT a flush guarantee. The second
    // call then exercises the pre-loop `matches()` shortcut deterministically.
    await pty.waitForLine((line) => VERSION_LINE.test(line), RESOLVE_WAIT_MS);
    await pty.exited;
    const output = await pty.waitForLine((line) => line.includes(VERSION_NAME), RESOLVE_WAIT_MS);
    expect(output.includes(VERSION_NAME)).toBe(true);
  }, SPAWN_TEST_TIMEOUT_MS);

  test("matches a help banner line emitted by a different command", async () => {
    const pty = spawnPty(HELP_ARGS);
    const output = await pty.waitForLine((line) => line.startsWith(HELP_USAGE_PREFIX), RESOLVE_WAIT_MS);
    expect(output.includes(HELP_USAGE_PREFIX)).toBe(true);
    await pty.exited;
  }, SPAWN_TEST_TIMEOUT_MS);
});

describe("PtyProcess — waitForLine timeout", () => {
  test("rejects with a timed-out message on a never-matching predicate", async () => {
    const pty = spawnPty(VERSION_ARGS);
    let message: string | null = null;
    try {
      await pty.waitForLine(NEVER_MATCH, REJECT_WAIT_MS);
    } catch (err) {
      message = (err as Error).message;
    }
    expect(message).not.toBeNull();
    expect(String(message).includes(TIMED_OUT_MARKER)).toBe(true);
    await pty.exited;
  }, REJECT_TEST_TIMEOUT_MS);

  test("a rejected waiter does not strand the suite — exited still resolves", async () => {
    const pty = spawnPty(VERSION_ARGS);
    await pty.waitForLine(NEVER_MATCH, REJECT_WAIT_MS).catch(() => undefined);
    const code = await pty.exited;
    expect(code).toBe(EXIT_OK);
  }, REJECT_TEST_TIMEOUT_MS);
});

describe("PtyProcess — output normalisation", () => {
  test("strips carriage returns the pty echoes", async () => {
    const pty = spawnPty(VERSION_ARGS);
    await pty.waitForLine((line) => VERSION_LINE.test(line), RESOLVE_WAIT_MS);
    await pty.exited;
    expect(pty.output.includes(CARRIAGE_RETURN)).toBe(false);
    expect(VERSION_LINE.test(pty.output)).toBe(true);
  }, SPAWN_TEST_TIMEOUT_MS);
});

describe("PtyProcess — exit code propagation", () => {
  test("exited resolves to 0 for --version", async () => {
    const pty = spawnPty(VERSION_ARGS);
    const code = await pty.exited;
    expect(code).toBe(EXIT_OK);
  }, SPAWN_TEST_TIMEOUT_MS);

  test("exited resolves to the usage-error code for an unknown command", async () => {
    const pty = spawnPty(UNKNOWN_ARGS);
    const code = await pty.exited;
    expect(code).toBe(EXIT_UNKNOWN_COMMAND);
  }, SPAWN_TEST_TIMEOUT_MS);

  test("kill resolves exited instead of leaking a child", async () => {
    const pty = spawnPty(HELP_ARGS);
    pty.kill();
    const code = await pty.exited;
    expect(typeof code).toBe("number");
  }, SPAWN_TEST_TIMEOUT_MS);
});
