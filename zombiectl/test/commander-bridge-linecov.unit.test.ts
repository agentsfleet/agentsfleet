// Line-coverage backfill for commander-bridge.ts. runCommanderParse is
// reached only through cli.ts in the integration suites, where the program
// is the fully-built CLI. Driving it there never exercises the non-Commander
// failure branch (a handler throwing a plain Error) — commander's own parse
// validation short-circuits everything before a handler runs. These tests
// invoke the exported runCommanderParse directly with hand-built commander
// programs so each parse outcome (plain-error failure, CommanderError
// failure, clean success) fires as a callable unit. runCommanderParse
// self-provides its parse-only layer, so no in-memory layers are needed.

import { describe, expect, test } from "bun:test";
import { Effect } from "effect";
import { Command, CommanderError } from "commander";
import { runCommanderParse } from "../src/lib/commander-bridge.ts";

const PLAIN_ERROR_MESSAGE = "handler exploded";
const UNKNOWN_SUBCOMMAND = "definitely-not-a-real-command";

// Build a program with exitOverride so commander throws instead of calling
// process.exit, and one action subcommand whose body we control per-test.
function programWith(
  subcommand: string,
  action: () => void,
): Command {
  const program = new Command("zombiectl");
  program.exitOverride();
  program.command(subcommand).action(action);
  return program;
}

describe("runCommanderParse parse-stage failure classification", () => {
  test("a handler throwing a plain Error surfaces as otherError, not commanderError", async () => {
    const program = programWith("boom", () => {
      throw new Error(PLAIN_ERROR_MESSAGE);
    });

    const result = await Effect.runPromise(
      runCommanderParse(program, ["boom"]),
    );

    // The non-CommanderError branch: ok=false, commanderError cleared,
    // otherError carries the thrown value verbatim.
    expect(result.ok).toBe(false);
    expect(result.commanderError).toBeUndefined();
    expect(result.otherError).toBeInstanceOf(Error);
    expect((result.otherError as Error).message).toBe(PLAIN_ERROR_MESSAGE);
    expect(result.otherError instanceof CommanderError).toBe(false);
  });

  test("a non-Error thrown value (string) still routes through otherError", async () => {
    const program = programWith("rejectstr", () => {
      // commander rejects parseAsync with whatever the action throws.
      // oxlint-disable-next-line no-throw-literal -- intentional non-Error throw exercises the otherError routing path
      throw PLAIN_ERROR_MESSAGE;
    });

    const result = await Effect.runPromise(
      runCommanderParse(program, ["rejectstr"]),
    );

    expect(result.ok).toBe(false);
    expect(result.commanderError).toBeUndefined();
    expect(result.otherError).toBe(PLAIN_ERROR_MESSAGE);
  });

  test("an unknown command surfaces as a CommanderError with otherError cleared", async () => {
    const program = programWith("known", () => {});

    const result = await Effect.runPromise(
      runCommanderParse(program, [UNKNOWN_SUBCOMMAND]),
    );

    // The CommanderError branch: ok=false, commanderError populated,
    // otherError cleared. This is the sibling branch to the one above.
    expect(result.ok).toBe(false);
    expect(result.otherError).toBeUndefined();
    expect(result.commanderError).toBeInstanceOf(CommanderError);
  });
});

describe("runCommanderParse success path", () => {
  test("a clean parse reports ok with both error slots cleared", async () => {
    let ran = false;
    const program = programWith("ok", () => {
      ran = true;
    });

    const result = await Effect.runPromise(
      runCommanderParse(program, ["ok"]),
    );

    // The success return object: the handler ran and no failure was recorded.
    expect(ran).toBe(true);
    expect(result.ok).toBe(true);
    expect(result.commanderError).toBeUndefined();
    expect(result.otherError).toBeUndefined();
  });
});
