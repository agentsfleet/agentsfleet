import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

// Vocab guard: the user-facing product noun is "agent". The retired entity noun
// "zombie" — retired in the entity cutover — must never resurface as a bare
// English word in *rendered copy*. This scan extracts user-visible text (string
// literals + JSX text) and fails if a multi-word phrase contains the standalone
// word "zombie", so a future regression can't silently reintroduce it.
//
// Scope is deliberately narrow to avoid false positives:
//   - only string literals ("…", '…', `…`) and same-line JSX text (>…<)
//   - only multi-word phrases (a space) — single tokens are
//     identifiers / paths / test-ids / class names / enum values, not copy
//   - comment lines are skipped
//   - brand/route/schema fragments are allowlisted
const ALLOW_FRAGMENTS: string[] = []; // zombie is fully retired — nothing legit to skip
const WORD = /\bzombies?\b/i;

const SEGMENT = /"([^"]*)"|'([^']*)'|`([^`]*)`|>([^<>{}]*)</g;

const ROOTS = [join(__dirname, "..", "app"), join(__dirname, "..", "components")];

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) out.push(...walk(p));
    else if (/\.(tsx|ts)$/.test(entry) && !/\.test\.(tsx|ts)$/.test(entry)) out.push(p);
  }
  return out;
}

function isComment(line: string): boolean {
  const t = line.trim();
  return t.startsWith("//") || t.startsWith("*") || t.startsWith("/*");
}

describe("vocab guard — no user-facing 'zombie' product noun", () => {
  it("rendered copy names the agent, never the retired noun 'zombie'", () => {
    const offenders: string[] = [];
    for (const root of ROOTS) {
      for (const file of walk(root)) {
        readFileSync(file, "utf8")
          .split("\n")
          .forEach((line, i) => {
            if (isComment(line)) return;
            for (const m of line.matchAll(SEGMENT)) {
              const seg = m[1] ?? m[2] ?? m[3] ?? m[4] ?? "";
              if (!seg.includes(" ")) continue; // single token = identifier, not copy
              if (!WORD.test(seg)) continue;
              if (ALLOW_FRAGMENTS.some((f) => seg.includes(f))) continue;
              offenders.push(`${file.split("/app/").pop()}:${i + 1}: "${seg.trim()}"`);
            }
          });
      }
    }
    expect(offenders, `user-facing "zombie" copy found:\n${offenders.join("\n")}`).toEqual([]);
  });
});
