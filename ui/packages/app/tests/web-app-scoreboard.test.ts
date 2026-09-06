import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// docs/architecture/web_app.md carries a scoreboard whose rows are each "one
// grep". This pins the row this milestone moved to the tree it describes, so
// the number and the code cannot drift apart silently.

const APP_ROOT = join(import.meta.dirname, "..");
const SCOREBOARD_PATH = join(APP_ROOT, "..", "..", "..", "docs", "architecture", "web_app.md");
const SCANNED_DIRECTORIES = ["app", "components"] as const;
const HOOK = "useOptimistic";
const SCOREBOARD_ROW = /^\| `useOptimistic` \| (\d+) \|/m;

/** `grep -rl <hook> app components | wc -l`, as the doc's own row spells it. */
function filesMentioning(hook: string): number {
  let count = 0;
  const walk = (directory: string) => {
    for (const entry of readdirSync(directory)) {
      const path = join(directory, entry);
      if (statSync(path).isDirectory()) walk(path);
      else if (readFileSync(path, "utf8").includes(hook)) count += 1;
    }
  };
  for (const directory of SCANNED_DIRECTORIES) walk(join(APP_ROOT, directory));
  return count;
}

describe("web_app.md scoreboard", () => {
  it("the scoreboard useOptimistic row equals the grep", () => {
    const row = SCOREBOARD_ROW.exec(readFileSync(SCOREBOARD_PATH, "utf8"));
    expect(row, "the scoreboard has no useOptimistic row").not.toBeNull();
    expect(Number(row?.[1])).toBe(filesMentioning(HOOK));
  });
});
