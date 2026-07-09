import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

// Regression guard for the M120 label sweep: the model surfaces renamed
// "Model id" → "Model" and "Key name" → "Name". This scans the dashboard app
// tree for either stale label so a future edit reintroducing them fails here,
// not in a design review. Test files are excluded — they legitimately assert on
// prior wording and their own fixture strings.
const APP_ROOT = join(__dirname, "..", "app");
const STALE_LABELS = ["Model id", "Key name"] as const;

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === "node_modules") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...sourceFiles(full));
      continue;
    }
    if (!/\.(tsx?|jsx?)$/.test(entry)) continue;
    if (/\.(test|spec)\.[tj]sx?$/.test(entry)) continue;
    out.push(full);
  }
  return out;
}

describe("no stale model labels under app/", () => {
  it("contains zero 'Model id' or 'Key name' labels in dashboard source", () => {
    const offenders: string[] = [];
    for (const file of sourceFiles(APP_ROOT)) {
      const text = readFileSync(file, "utf8");
      for (const label of STALE_LABELS) {
        if (text.includes(label)) offenders.push(`${file}: "${label}"`);
      }
    }
    expect(offenders).toEqual([]);
  });
});
