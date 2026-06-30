// Line-coverage backfill for runCli's non-CommanderError parse-failure
// tail (src/cli.ts). The existing exit-code suite drives the reachable
// CommanderError branches (help → 0, state.exitCode short-circuit → 1,
// usage codes → 2). This file drives the complementary branch: when a
// raw (non-CommanderError) Error escapes commander's parseAsync, the
// commander-bridge captures it as `otherError` and runCli's fallthrough
// runs — `errMessage(err)` extracts the message and the envelope is
// emitted as text (default) or JSON (--json).
//
// How a raw Error is forced without editing source: runCli wires
// `program.configureOutput({ writeOut, writeErr })` so commander's help
// renderer writes through the injected stdout. A stdout whose `.write`
// throws a plain Error makes commander throw mid-help — a non-Commander
// throw — which routes through the bridge to the fallthrough. The
// command's own stderr stays clean, so it captures the envelope runCli
// writes. We assert the captured message is the thrown Error's message
// (proving errMessage read `err.message` off an Error instance) and that
// the exit code is the unexpected-error code 1.
//
// Reached this way: the `errMessage` Error branch and runCli's text +
// JSON fallthrough writes, plus their shared `return 1` tail.

import { describe, test, expect } from "bun:test";
import { Writable } from "node:stream";

import { runCli } from "../src/cli.ts";
import { bufferStream, withFreshStateDir } from "./helpers-cli-state.ts";

const BOOM = "simulated stdout write failure";
const HELP_ARG = "--help";
const NO_COLOR_ENV = { NO_COLOR: "1" } as const;

// A stdout whose first (and every) write throws a plain Error. Commander
// renders --help through this stream during parseAsync, so the throw
// surfaces as a non-CommanderError the bridge reports as `otherError`.
function throwingStdout(message: string): Writable & { isTTY?: boolean } {
  return new Writable({
    write() {
      throw new Error(message);
    },
  }) as Writable & { isTTY?: boolean };
}

// Sibling of throwingStdout that throws a NON-Error value (a bare string).
// The bridge still reports it as `otherError`, but `errMessage` falls through
// its `instanceof Error` guard and takes the `String(err)` branch — the
// complementary line to the Error path above.
function throwingStdoutNonError(value: string): Writable & { isTTY?: boolean } {
  return new Writable({
    write() {
      // eslint-disable-next-line no-throw-literal
      throw value;
    },
  }) as Writable & { isTTY?: boolean };
}

describe("runCli non-CommanderError parse-failure tail", () => {
  test("a raw Error escaping parse renders error: <message> on stderr and exits 1", async () => {
    await withFreshStateDir(async () => {
      const err = bufferStream();
      const code = await runCli([HELP_ARG], {
        stdout: throwingStdout(BOOM),
        stderr: err.stream,
        env: NO_COLOR_ENV,
      });

      // errMessage(err) pulled the message off the Error instance, and
      // the non-JSON fallthrough wrote it through ui.err as `error: …`.
      expect(code).toBe(1);
      expect(err.read()).toContain(`error: ${BOOM}`);
    });
  });

  test("the same failure under --json emits an UNEXPECTED error envelope and exits 1", async () => {
    await withFreshStateDir(async () => {
      const err = bufferStream();
      const code = await runCli(["--json", HELP_ARG], {
        stdout: throwingStdout(BOOM),
        stderr: err.stream,
        env: NO_COLOR_ENV,
      });

      // JSON fallthrough: printJson(stderr, { error: { code, message } }).
      // The message is again the thrown Error's message via errMessage.
      expect(code).toBe(1);
      const envelope = JSON.parse(err.read()) as {
        error: { code: string; message: string };
      };
      expect(envelope.error.code).toBe("UNEXPECTED");
      expect(envelope.error.message).toBe(BOOM);
    });
  });

  test("the thrown Error's own message — not a stringified fallback — reaches the operator", async () => {
    // Distinct sentinel: if errMessage took the `String(err)` fallback
    // (non-Error branch) the operator would see "Error: …" framing rather
    // than the bare message. Asserting the bare message proves the
    // Error-instance branch ran.
    const distinct = "distinct write-path failure marker";
    await withFreshStateDir(async () => {
      const err = bufferStream();
      const code = await runCli([HELP_ARG], {
        stdout: throwingStdout(distinct),
        stderr: err.stream,
        env: NO_COLOR_ENV,
      });

      expect(code).toBe(1);
      const rendered = err.read();
      expect(rendered).toContain(distinct);
      expect(rendered).not.toContain(`Error: ${distinct}`);
    });
  });

  test("a non-Error thrown value routes through errMessage's String(err) fallback", async () => {
    // Complementary to the Error-instance branch: a bare string thrown from
    // the write path is not `instanceof Error`, so errMessage returns
    // String(err) — the raw string reaches the operator as `error: <string>`.
    const marker = "non-error string thrown from write";
    await withFreshStateDir(async () => {
      const err = bufferStream();
      const code = await runCli([HELP_ARG], {
        stdout: throwingStdoutNonError(marker),
        stderr: err.stream,
        env: NO_COLOR_ENV,
      });

      expect(code).toBe(1);
      expect(err.read()).toContain(`error: ${marker}`);
    });
  });
});
