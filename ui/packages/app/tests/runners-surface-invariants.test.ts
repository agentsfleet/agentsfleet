import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// The runner surface's grep-enforced invariants: one failure vocabulary, one
// route-string producer, and no surviving reference to the retired table.

const APP_ROOT = join(__dirname, "..");
const RUNNERS_DIR = join(APP_ROOT, "app", "(dashboard)", "admin", "runners");

function sourceFilesUnder(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      out.push(...sourceFilesUnder(path));
      continue;
    }
    if (/\.(ts|tsx)$/.test(entry) && !/\.test\./.test(entry)) out.push(path);
  }
  return out;
}

describe("runner surface invariants", () => {
  it("no runner component spells a failure tag literal", () => {
    // The shared vocabulary in lib/events/event-summary.ts is the only source
    // of failure copy; a tag literal here would be a second vocabulary.
    const tags = ["oom_kill", "timeout_kill", "transport_loss", "renewal_terminate", "startup_posture"];
    for (const file of sourceFilesUnder(RUNNERS_DIR)) {
      const source = readFileSync(file, "utf8");
      for (const tag of tags) {
        expect(source.includes(tag), `${file} spells ${tag}`).toBe(false);
      }
    }
  });

  it("the route string is written once, in runner-routes", () => {
    for (const file of sourceFilesUnder(RUNNERS_DIR)) {
      const source = readFileSync(file, "utf8");
      expect(source.includes('"/admin/runners'), `${file} inlines the runner path`).toBe(false);
    }
  });

  it("test_no_orphaned_runner_table_references", () => {
    // Word-boundary matches, mirroring the spec's `grep -w`. The runner-named
    // symbols must be gone app-wide; the generic cell names are checked only
    // on the runner surface, because other surfaces legitimately own
    // same-named components (models registry StatusCell, api-keys
    // KeyActionsCell) that were never the retired table's cells.
    const deletedEverywhere = ["RunnerActivityDialog", "RunnerList", "RunnerListHandle"];
    const deletedCells = ["HostCell", "StatusCell", "LabelsCell", "ActionsCell"];
    const roots = [join(APP_ROOT, "app"), join(APP_ROOT, "components"), join(APP_ROOT, "lib")];
    const referencesSymbol = (source: string, symbol: string) => new RegExp(`\\b${symbol}\\b`).test(source);
    for (const root of roots) {
      for (const file of sourceFilesUnder(root)) {
        const source = readFileSync(file, "utf8");
        for (const symbol of deletedEverywhere) {
          expect(referencesSymbol(source, symbol), `${file} references ${symbol}`).toBe(false);
        }
      }
    }
    for (const file of sourceFilesUnder(RUNNERS_DIR)) {
      const source = readFileSync(file, "utf8");
      for (const symbol of deletedCells) {
        expect(referencesSymbol(source, symbol), `${file} references ${symbol}`).toBe(false);
      }
    }
  });
});
