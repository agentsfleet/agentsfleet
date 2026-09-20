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
 * and a scanner that counted it would cut the envelope in half. Quotes the JSON
 * escaped are ignored too — see `escapedQuote`, which is the one thing reading
 * right-to-left makes harder rather than easier.
 */

import assert from "node:assert/strict";

const OPEN_BRACE = "{" as const;
const CLOSE_BRACE = "}" as const;
const QUOTE = '"' as const;
const BACKSLASH = "\\" as const;

/** Is the quote at `i` escaped — preceded by an ODD run of backslashes?
 *
 *  A forward scanner learns this for free: it meets the backslash first and
 *  carries a flag one character. Reading right-to-left the order inverts, so the
 *  quote has to be judged before its escape is ever seen, and the only evidence
 *  is the run of backslashes behind it. Parity is the whole answer — `"…\\"`
 *  closes a string because the pair escapes itself, `"…\""` does not.
 *
 *  Envelope fields carry free text a person wrote (`proposed_action` is the one
 *  that bites), so an escaped quote here is ordinary input, not a malformed
 *  stream. */
function escapedQuote(text: string, i: number): boolean {
  let backslashes = 0;
  for (let j = i - 1; j >= 0 && text[j] === BACKSLASH; j--) backslashes++;
  return backslashes % 2 === 1;
}

/** The trailing balanced `{…}` object in a stream the CLI also wrote prose to,
 *  as TEXT.
 *
 *  Named for what it returns. `trailingJsonObject` returned this same string,
 *  and a caller writing `trailingJsonObject(out) as { items?: … }` got a
 *  string wearing an object's type — TypeScript admits that cast, because a
 *  target whose properties are all optional is comparable to anything, so
 *  every field read back `undefined` and the assertion failed against output
 *  that was in fact correct. */
export function trailingJsonText(stdout: string): string {
  const end = stdout.lastIndexOf(CLOSE_BRACE);
  assert.ok(end >= 0, `steer --json produced no JSON object: ${stdout}`);
  let depth = 0;
  let inString = false;
  for (let i = end; i >= 0; i--) {
    const ch = stdout[i];
    if (ch === QUOTE && !escapedQuote(stdout, i)) {
      inString = !inString;
      continue;
    }
    if (inString) continue;
    if (ch === CLOSE_BRACE) depth++;
    else if (ch === OPEN_BRACE) {
      depth--;
      if (depth === 0) return stdout.slice(i, end + 1);
    }
  }
  throw new assert.AssertionError({ message: `unbalanced JSON in steer stdout: ${stdout}` });
}

/** The same trailing object, PARSED.
 *
 *  Returns `unknown` so a caller's `as` is a narrowing of a parsed value
 *  rather than a reinterpretation of a string. */
export function trailingJson(stdout: string): unknown {
  return JSON.parse(trailingJsonText(stdout));
}
