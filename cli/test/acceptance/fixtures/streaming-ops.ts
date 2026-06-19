/**
 * Streaming/long-lived helpers for the `steer`-REPL acceptance group
 * (streaming-follow.spec.ts). Owned solely by that spec.
 *
 * These centralize the wire/output literals the streamed `steer` REPL emits
 * so a render-format rename surfaces here once (RULE UFS), and they encode the
 * "clean exit" contract for a pty-interrupted long-lived command.
 *
 * Confirmed against source (do not re-guess):
 *   - src/lib/repl.ts            — STEER_REPL_PROMPT = "> "; SIGINT → exit 130.
 *   - src/commands/agent_steer_events.ts — content frames render as
 *                                   `[claw] …` / `[tool] …`.
 *   - src/commands/agent_steer.ts — terminal line `event <id> processed`
 *                                   (success) or `event <id> terminated with
 *                                   status: …` / `did not complete (…)`.
 *   - src/errors/index.ts        — InterruptedError → exit 130 (SIGINT).
 */

import assert from "node:assert/strict";

export const STEER_COMMAND = "steer" as const;

// The REPL prompt printed before each turn (src/lib/repl.ts STEER_REPL_PROMPT).
// Matched at the start of a line; readline re-prints it after every turn.
// `m` (not `g`) anchors to a line start — correct for `waitForLine`, which
// splits the transcript and tests each line, but it is the WRONG flag for
// counting: `String.match` on a non-global regex returns at most one element,
// so re-prompt counting must use `STEER_REPL_PROMPT_COUNT_RE` below instead.
export const STEER_REPL_PROMPT_RE = /^>\s?/m;

// Global+multiline twin of the prompt regex, used solely to COUNT re-prompts
// across the whole transcript (two+ ⇒ the loop re-prompted ⇒ it stayed alive
// rather than one-shotting). Kept separate from STEER_REPL_PROMPT_RE because
// `String.match` collapses a non-global regex to a single hit.
export const STEER_REPL_PROMPT_COUNT_RE = /^>\s?/gm;

// Streamed content frames emitted mid-turn (agent_steer_events.ts).
export const STEER_STREAM_FRAME_RE = /\[(?:claw|tool)\]/g;

// Terminal per-turn render (agent_steer.ts renderOutcome, non-JSON mode):
//   success  → `event <id> processed`
//   terminal → `event <id> terminated with status: …`
//   timeout  → `event <id> still in flight after …`
//   incomplete → `event <id> did not complete (…)`
// Any of these means a turn reached a terminal/streamed conclusion.
export const STEER_TURN_DONE_RE =
  /event\s+\S+\s+(?:processed|terminated with status|still in flight|did not complete)/gi;

// Exit codes that count as a clean Ctrl-C teardown of the streamed REPL:
//   130 — InterruptedError, the conventional SIGINT code (src/errors/index.ts)
//     0 — the REPL drained the final turn and closed before SIGINT landed.
export const STREAM_CLEAN_EXIT_CODES: ReadonlyArray<number> = [0, 130];

// Substrings that betray an UNCLEAN exit — a raw stack trace or an
// unhandled rejection/exception leaking through instead of a typed CliError.
const UNCLEAN_EXIT_MARKERS: ReadonlyArray<string> = [
  "    at ", // V8 stack-frame line prefix
  "UnhandledPromiseRejection",
  "Unhandled error",
  "unhandledRejection",
  "uncaughtException",
  "TypeError:",
  "ReferenceError:",
];

/**
 * True once a steer turn has visibly landed in the transcript — at least one
 * streamed `[claw]`/`[tool]` frame, OR the terminal `event <id> …` line for a
 * fast/terse reply that produced no intermediate frames. Use as the
 * `waitForLine` predicate (the harness applies it per accumulated line, but
 * the terminal regex also matches a single concluding line).
 */
export function steerTurnLanded(line: string): boolean {
  return new RegExp(STEER_STREAM_FRAME_RE.source).test(line)
    || new RegExp(STEER_TURN_DONE_RE.source, "i").test(line);
}

/**
 * Count REPL prompts emitted across the whole transcript. Two or more proves
 * the loop re-prompted after a turn (stayed alive) instead of one-shotting.
 * Uses the global prompt regex so every occurrence is counted — `String.match`
 * on the non-global STEER_REPL_PROMPT_RE would silently report just one.
 */
export function countSteerReprompts(transcript: string): number {
  return transcript.match(STEER_REPL_PROMPT_COUNT_RE)?.length ?? 0;
}

/**
 * Count streamed-content + terminal frames across the whole transcript. Each
 * turn contributes at least one (`[claw]`/`[tool]` frame, or a terminal
 * `event <id> …` line). Monotonic growth across two sends proves a new,
 * second turn was accepted and streamed in the SAME long-lived process.
 */
export function countSteerTurnFrames(transcript: string): number {
  const frames = transcript.match(STEER_STREAM_FRAME_RE)?.length ?? 0;
  const terminals = transcript.match(STEER_TURN_DONE_RE)?.length ?? 0;
  return frames + terminals;
}

/**
 * Assert the streamed REPL exited cleanly after Ctrl-C: an allowed exit code
 * AND no stack trace / unhandled-rejection noise in the merged pty output.
 */
export function assertCleanStreamExit(exitCode: number, transcript: string): void {
  assert.ok(
    STREAM_CLEAN_EXIT_CODES.includes(exitCode),
    `Ctrl-C must exit cleanly (${STREAM_CLEAN_EXIT_CODES.join(" or ")}); got ${exitCode}; output=${transcript}`,
  );
  for (const marker of UNCLEAN_EXIT_MARKERS) {
    assert.ok(
      !transcript.includes(marker),
      `clean exit must not leak "${marker}" (stack trace / unhandled error); output=${transcript}`,
    );
  }
}
