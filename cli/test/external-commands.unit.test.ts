// The suite's external dependencies, named before anything spawns one.
//
// This file exists because of how the absence of `zsh` presented: the
// completions case spawns `zsh -n` to parse the script we emit, GitHub's
// `ubuntu-latest` does not ship zsh, and `enforce-coverage.mjs` buffers bun's
// whole output into one write that GitHub then clipped. The lane went red with
// no `(fail)` line surviving in the log. One test that names the missing
// command costs nothing and would have answered it in a sentence.

import { describe, expect, test } from "bun:test";

import {
  OPTIONAL_COMMANDS,
  REQUIRED_COMMANDS,
  commandPresent,
  missingFrom,
} from "./helpers-external-commands.ts";

describe("the commands this suite shells out to", () => {
  test("every required command resolves, and the failure names the missing one", () => {
    const missing = missingFrom(REQUIRED_COMMANDS);
    expect(missing).toEqual([]);
  });

  test("an optional command is reported rather than assumed", () => {
    // Not an assertion about presence — an assertion that the answer is
    // knowable. A case guarded on one of these skips with a reason instead of
    // failing on a spawn.
    for (const name of OPTIONAL_COMMANDS) expect(typeof commandPresent(name)).toBe("boolean");
  });

  test("the probe answers false for a command no box has", () => {
    expect(commandPresent("a-command-no-box-carries-27f3")).toBe(false);
  });
});
