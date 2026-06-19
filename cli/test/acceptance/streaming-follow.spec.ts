/**
 * streaming-follow — long-lived / streamed `steer` against a live agent,
 * driven through a real pseudo-terminal (GROUP 4).
 *
 * Why a pty, and why `steer` (not `logs --follow`):
 *   - The `agentsfleet logs` command is one-shot paginated: cli-tree-agent.ts
 *     defines `logs [agent_id]` with ONLY `--agent` / `--limit` / `--cursor`.
 *     There is NO `--follow`/`--tail` flag and no live-tail loop in
 *     src/commands/agent_logs.ts — a `--follow` test would assert against an
 *     API that does not exist. The genuine long-lived, streamed surface in
 *     the CLI is `steer` ("Send a message; stream the response").
 *   - `steer <id>` with NO positional message and a TTY stdin enters the
 *     interactive REPL (`shouldEnterSteerRepl` = message===undefined && tty,
 *     src/lib/repl.ts). The REPL is the long-lived loop: it prints the
 *     `STEER_REPL_PROMPT` ("> "), reads a line, streams the turn as
 *     `[claw] …` / `[tool] …` content frames, prints a terminal
 *     `event <id> processed` line (or a tolerated terminal/timeout stem on a
 *     shared DEV tenant), then RE-PROMPTS and waits for the next line. That
 *     re-prompt is exactly the "appended rather than one-shotting" property.
 *   - `runAgentctl` pipes stdin (non-TTY), so the REPL never engages there;
 *     a real pty (`PtyProcess.spawnAgentctl` via pty-spawn.py) is the only way
 *     to exercise the long-lived path. Under a pty the child's stdout AND
 *     stderr both land on the pty slave, so everything arrives on `output`.
 *
 * Scenarios:
 *   (a) live tail + clean Ctrl-C exit — spawn `steer <id>` (REPL), drive one
 *       turn, wait for at least the initial streamed output line, confirm the
 *       REPL stayed alive (re-prompted) rather than one-shotting, then
 *       interrupt() (Ctrl-C). Assert a CLEAN exit: code 130 (InterruptedError,
 *       the conventional SIGINT code per src/errors/index.ts) or 0, and NO
 *       stack trace / unhandled-rejection noise in the combined output.
 *   (b) multi-turn streaming — send a FIRST message, await streamed output,
 *       then send a SECOND message to the SAME agent in the SAME REPL and
 *       assert the second turn is accepted (a second streamed/terminal frame
 *       arrives). Then interrupt cleanly.
 *
 * Teardown: prefix-scoped `cleanWorkspaceAgents` — only this run's agents are
 * killed; shared-tenant residue from other runs is untouched and global
 * emptiness is never asserted.
 *
 * The minted JWT must never appear in any pty output (`assertNoSecretLeak`).
 *
 * Live-only: registers real tests only when `AGENTSFLEET_ACCEPTANCE_TARGET`
 * is an https URL; otherwise every test is skipped cleanly (no target → skip).
 */

import { describe, it, beforeAll, afterAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { ACCEPTANCE_RUN_PREFIX, ACCEPTANCE_TARGET_ENV } from "./fixtures/constants.ts";
import { composeEnv } from "./fixtures/cli.js";
import { assertNoSecretLeak } from "./fixtures/negatives.ts";
import {
  resolveAcceptanceEnv,
  resolveClerkSecret,
  resolveFixtureEmail,
} from "./global-setup.ts";
import { attachJwt } from "./fixtures/clerk-admin.ts";
import { hydrateWorkspacesForToken } from "./fixtures/workspace-hydration.ts";
import { installPlatformOpsAgent } from "./fixtures/seed.ts";
import { cleanWorkspaceAgents } from "./fixtures/teardown.ts";
import { PtyProcess } from "./fixtures/pty.ts";
import {
  STEER_COMMAND,
  STEER_REPL_PROMPT_RE,
  steerTurnLanded,
  countSteerReprompts,
  countSteerTurnFrames,
  assertCleanStreamExit,
} from "./fixtures/streaming-ops.ts";

const target = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
const isLive = target.startsWith("https://");

// Wire / output literals (RULE UFS — each repeats or crosses a boundary).
const STATE_DIR_PREFIX = "agentsfleet-streaming-" as const;
const NO_COLOR = "1" as const;
const FIRST_MESSAGE = "respond with a single short acknowledgement and stop" as const;
const SECOND_MESSAGE = "now reply with just the word done" as const;

// The streamed SSE round-trip falls back to a ~60s poll window before it
// declares a turn terminal. Budget each per-turn wait well above that so a
// slow-but-valid turn on a shared DEV tenant still lands a frame in time.
const TURN_WAIT_MS = 180_000;

// Generous ceiling for the whole pty lifetime (install happens in beforeAll;
// these are the in-test waits). After interrupt(), the child must exit; bound
// the exit await so a hung child fails loud instead of hanging the suite.
const EXIT_WAIT_MS = 30_000;

if (!isLive) {
  describe("streaming-follow.spec.ts", () => {
    it.skip(`requires ${ACCEPTANCE_TARGET_ENV} to be an https URL`, () => {});
  });
} else {
  describe("streaming-follow — long-lived / streamed steer over a pty", () => {
    let sessionJwt = "";
    let stateDir = "";
    let env: Record<string, string> = {};
    let workspaceId = "";
    let agentId = "";

    beforeAll(async () => {
      const apiUrl = resolveAcceptanceEnv().apiUrl;
      const clerkSecret = resolveClerkSecret();
      const email = resolveFixtureEmail("regular");
      const minted = await attachJwt(clerkSecret, { email });
      sessionJwt = minted.sessionJwt;

      stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
      env = composeEnv({
        AGENTSFLEET_TOKEN: sessionJwt,
        AGENTSFLEET_API_URL: apiUrl,
        AGENTSFLEET_STATE_DIR: stateDir,
        NO_COLOR: NO_COLOR,
      });
      const hydrated = await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
      workspaceId = hydrated.currentWorkspaceId;

      const installed = await installPlatformOpsAgent({ env });
      const id = installed.id ?? installed.agent_id;
      if (!id) throw new Error(`install missing id: ${JSON.stringify(installed)}`);
      agentId = id;
    }, TURN_WAIT_MS);

    afterAll(async () => {
      if (env && workspaceId) {
        try {
          await cleanWorkspaceAgents(env, { workspaceId, runPrefix: ACCEPTANCE_RUN_PREFIX });
        } catch { /* best-effort teardown; never fail the run on cleanup */ }
      }
      if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    });

    // (a) Live tail of a streamed turn, then a clean Ctrl-C exit. The REPL is
    // the long-lived surface: it must re-prompt after the first turn (proof it
    // appended / stayed alive rather than one-shotting), and Ctrl-C must exit
    // cleanly with no stack trace.
    it("steer REPL streams a turn, stays alive, then exits cleanly on Ctrl-C", async () => {
      assert.ok(agentId, "agent was not installed in beforeAll");
      const cli = PtyProcess.spawnAgentctl([STEER_COMMAND, agentId], { env });
      try {
        // The REPL announces itself with the "> " prompt before any input.
        await cli.waitForLine((line) => STEER_REPL_PROMPT_RE.test(line), TURN_WAIT_MS);

        cli.writeLine(FIRST_MESSAGE);
        // Wait for the turn to land: at least one streamed `[claw]`/`[tool]`
        // frame, or the terminal `event <id> …` line on a fast/terse reply.
        await cli.waitForLine((line) => steerTurnLanded(line), TURN_WAIT_MS);

        // Long-lived proof: after the first turn the REPL re-prompts and keeps
        // running. A one-shot command would have exited here. The initial
        // prompt plus the post-turn re-prompt is >=2; readline re-prints the
        // prompt before reading the next line, so the second occurrence is the
        // "appended rather than one-shotting" signal. countSteerReprompts uses
        // the GLOBAL prompt regex — a non-global `.match` would cap at one.
        await cli.waitForLine(() => countSteerReprompts(cli.output) >= 2, TURN_WAIT_MS);
        assert.ok(
          countSteerReprompts(cli.output) >= 2,
          `REPL must re-prompt after a turn (stay alive, not one-shot); output=${cli.output}`,
        );

        cli.interrupt();
        const exitCode = await raceExit(cli.exited, EXIT_WAIT_MS);
        assertCleanStreamExit(exitCode, cli.output);
      } finally {
        cli.kill();
      }
      // pty merges stdout+stderr onto `output`; the JWT must never surface.
      assertNoSecretLeak({ stdout: cli.output, stderr: "" }, sessionJwt);
    }, TURN_WAIT_MS + EXIT_WAIT_MS);

    // (b) Multi-turn: a second message to the SAME agent in the SAME REPL is
    // accepted and streams its own turn.
    it("steer REPL accepts a second turn to the same agent (multi-turn)", async () => {
      assert.ok(agentId, "agent was not installed in beforeAll");
      const cli = PtyProcess.spawnAgentctl([STEER_COMMAND, agentId], { env });
      try {
        await cli.waitForLine((line) => STEER_REPL_PROMPT_RE.test(line), TURN_WAIT_MS);

        cli.writeLine(FIRST_MESSAGE);
        await cli.waitForLine((line) => steerTurnLanded(line), TURN_WAIT_MS);
        const afterFirst = countSteerTurnFrames(cli.output);
        assert.ok(afterFirst >= 1, `first turn produced no streamed/terminal frame; output=${cli.output}`);

        // Second turn to the SAME agent in the SAME long-lived process.
        cli.writeLine(SECOND_MESSAGE);
        await cli.waitForLine(() => countSteerTurnFrames(cli.output) > afterFirst, TURN_WAIT_MS);
        assert.ok(
          countSteerTurnFrames(cli.output) > afterFirst,
          `second turn was not accepted (no additional streamed/terminal frame); output=${cli.output}`,
        );

        cli.interrupt();
        const exitCode = await raceExit(cli.exited, EXIT_WAIT_MS);
        assertCleanStreamExit(exitCode, cli.output);
      } finally {
        cli.kill();
      }
      assertNoSecretLeak({ stdout: cli.output, stderr: "" }, sessionJwt);
    }, TURN_WAIT_MS * 2 + EXIT_WAIT_MS);

    // Await the child's exit, but fail loud if it hangs past `ms` after Ctrl-C
    // instead of stalling the whole suite. The timeout timer is CLEARED the
    // moment `exited` settles, so the loser of the race never rejects an
    // unobserved promise — a dangling `setTimeout(reject)` would surface as an
    // unhandledRejection, which is itself one of the UNCLEAN_EXIT_MARKERS this
    // suite guards against.
    function raceExit(exited: Promise<number>, ms: number): Promise<number> {
      return new Promise((resolve, reject) => {
        const timer = setTimeout(
          () => reject(new Error(`pty did not exit within ${ms}ms after Ctrl-C`)),
          ms,
        );
        exited.then(
          (code) => { clearTimeout(timer); resolve(code); },
          (err) => { clearTimeout(timer); reject(err instanceof Error ? err : new Error(String(err))); },
        );
      });
    }
  });
}
