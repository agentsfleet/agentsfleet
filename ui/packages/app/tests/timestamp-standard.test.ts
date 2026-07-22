import { readdirSync, readFileSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { describe, expect, it } from "vitest";

// The timestamp standard: every rendered timestamp goes through
// `Time` from @agentsfleet/design-system, which owns the <time datetime>
// semantic, the locale pin, the hydration guard, and the tooltip. Only two
// homes may compute a date string that is then fed to `Time` as a `label`:
// the design-system time-utils, and a domain formatter. Everything else is a
// bespoke formatter and must be deleted. This is a grep-based invariant — it
// goes red when a new bespoke date formatter appears in app production code,
// so the standard cannot regress silently.

// Resolve against THIS test file, never the process cwd — vitest may run from
// the monorepo root or the package dir.
const TESTS_DIR = path.dirname(fileURLToPath(import.meta.url));
const APP_ROOT = path.resolve(TESTS_DIR, ".."); // ui/packages/app

// Directories that never hold production source.
const SKIP_DIRS = new Set(["node_modules", ".next", "tests", "dist", ".turbo"]);

// The only files sanctioned to touch a locale date/number API in production:
//   - charges.ts      — the ledger "MMM DD, YYYY · HH:MM" label fed to Time.
//   - CatalogueList.tsx — formats a token COUNT (a number), not a date.
// Paths are relative to APP_ROOT with POSIX separators.
const ALLOWED = new Set([
  "app/(dashboard)/settings/billing/lib/charges.ts",
  "app/(dashboard)/admin/models/components/CatalogueList.tsx",
]);

// A line is a bespoke date formatter if it calls one of the locale date
// methods, or constructs an Intl date formatter for FORMATTING. The
// `Intl.DateTimeFormat().resolvedOptions().timeZone` idiom reads the caller's
// timezone name — it formats no date — so it is not a violation.
//
// Intl.RelativeTimeFormat is included deliberately: hand-rolling a "… ago"
// label is the one way to satisfy the letter of this guard while defeating its
// purpose. `Time format="relative"` is the only sanctioned relative renderer.
function isBespokeDateLine(line: string): boolean {
  if (/toLocaleString|toLocaleDateString|toLocaleTimeString/.test(line)) return true;
  if (/Intl\.RelativeTimeFormat/.test(line)) return true;
  if (/Intl\.DateTimeFormat/.test(line) && !/resolvedOptions/.test(line)) return true;
  return false;
}

function walk(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (SKIP_DIRS.has(entry)) continue;
      out.push(...walk(full));
      continue;
    }
    if (!/\.(ts|tsx)$/.test(entry)) continue;
    if (/\.test\.|\.spec\./.test(entry)) continue;
    out.push(full);
  }
  return out;
}

function relPosix(full: string): string {
  return path.relative(APP_ROOT, full).split(path.sep).join("/");
}

const SOURCE_FILES = walk(APP_ROOT);

describe("timestamp standard", () => {
  it("test_formatdate_deleted", () => {
    // The dead Intl.DateTimeFormat helper (lib/utils.ts) had zero callers and
    // is deleted outright. Build the needle dynamically so the literal token
    // never appears in this file — the repo-wide dead-code grep stays at 0.
    const needle = "format" + "Date";
    const re = new RegExp(`\\b${needle}\\b`);
    const offenders = SOURCE_FILES.filter((f) =>
      re.test(readFileSync(f, "utf8")),
    ).map(relPosix);
    expect(
      offenders,
      `The deleted helper is still referenced in:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });

  it("test_no_bespoke_date_formatters", () => {
    const offenders: string[] = [];
    for (const file of SOURCE_FILES) {
      const rel = relPosix(file);
      if (ALLOWED.has(rel)) continue;
      const hits = readFileSync(file, "utf8")
        .split("\n")
        .map((line, i) => ({ line, n: i + 1 }))
        .filter(({ line }) => isBespokeDateLine(line));
      for (const { n } of hits) offenders.push(`${rel}:${n}`);
    }
    expect(
      offenders,
      `Bespoke date formatter(s) outside the sanctioned homes — render through ` +
        `Time from @agentsfleet/design-system instead:\n${offenders.join("\n")}`,
    ).toEqual([]);
  });
});
