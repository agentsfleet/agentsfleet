/**
 * steer-envelope.ts — finding the JSON envelope in `steer --json` output.
 *
 * `steer` streams content frames to stdout as plain `[claw] …` / `[tool] …`
 * lines through `output.info` even under `--json`, and writes the envelope LAST
 * through `output.printJson`. So `JSON.parse(stdout)` throws on any turn that
 * streamed prose, and the envelope is the trailing balanced `{…}` object.
 *
 * Scanning backwards from the last `}` to its depth-zero `{`, ignoring braces
 * inside string literals: a model that prints a brace in its reply is ordinary,
 * and a scanner that counted it would cut the envelope in half.
 */

import assert from "node:assert/strict";

const OPEN_BRACE = "{" as const;
const CLOSE_BRACE = "}" as const;
const QUOTE = '"' as const;
const BACKSLASH = "\\" as const;

/** The trailing balanced `{…}` object in a stream the CLI also wrote prose to. */
export function trailingJsonObject(stdout: string): string {
  const end = stdout.lastIndexOf(CLOSE_BRACE);
  assert.ok(end >= 0, `steer --json produced no JSON object: ${stdout}`);
  let depth = 0;
  let inString = false;
  let escaped = false;
  for (let i = end; i >= 0; i--) {
    const ch = stdout[i];
    if (escaped) { escaped = false; continue; }
    if (inString) {
      if (ch === BACKSLASH) { escaped = true; continue; }
      if (ch === QUOTE) inString = false;
      continue;
    }
    if (ch === QUOTE) { inString = true; continue; }
    if (ch === CLOSE_BRACE) depth++;
    else if (ch === OPEN_BRACE) {
      depth--;
      if (depth === 0) return stdout.slice(i, end + 1);
    }
  }
  throw new assert.AssertionError({ message: `unbalanced JSON in steer stdout: ${stdout}` });
}
