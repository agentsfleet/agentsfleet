import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Every action button under the dashboard follows one rule: `default` (no
 * variant prop — the mint CTA) for the ONE primary action on a surface,
 * `ghost` for every other non-destructive action, `destructive` for a
 * destructive one, and `link` only inline in prose or a table cell.
 *
 * The walk that raised this counted "8 pages"; the brief that followed
 * counted 48 files by grepping `variant="…"`, which also matched every Alert
 * and Badge. A scanner that stops at the first `>` after `<Button` — the one
 * inside `onClick={() => …}` — miscounted three more. So this test walks the
 * tag to its real end, skipping braces and quotes, and names each offender
 * by file and line rather than reporting a count.
 */

const DASHBOARD_ROOT = join(__dirname, "..", "app", "(dashboard)");
const BUTTON_OPEN = "<Button";
const TEST_FILE_MARK = ".test.";

// The variants the rule permits as literals. A tag with no variant renders
// `default`, which is the primary and is permitted by omission.
const PERMITTED_LITERAL_VARIANTS: ReadonlySet<string> = new Set(["ghost", "destructive", "link"]);

// Sites whose variant is decided by a per-action config (`variant={…}`); the
// rule is audited on that config, not on the site.
const CONFIG_DRIVEN_SITES: ReadonlySet<string> = new Set([
  "admin/runners/[runnerId]/components/RunnerHeader.tsx",
  "w/[workspaceId]/fleets/[id]/components/KillSwitch.tsx",
]);

function tsxFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return tsxFiles(path);
    return name.endsWith(".tsx") && !name.includes(TEST_FILE_MARK) ? [path] : [];
  });
}

/** Index of the `>` that closes the opening tag starting at `from`, honouring `{…}` and strings. */
function tagEnd(source: string, from: number): number {
  let depth = 0;
  let quote: string | null = null;
  for (let i = from; i < source.length; i += 1) {
    const c = source[i];
    if (quote) {
      if (c === "\\") i += 1;
      else if (c === quote) quote = null;
    } else if (c === '"' || c === "'" || c === "`") quote = c;
    else if (c === "{") depth += 1;
    else if (c === "}") depth -= 1;
    else if (c === ">" && depth === 0) return i;
  }
  return -1;
}

type Offender = { site: string; variant: string };

function offendersIn(path: string): Offender[] {
  const source = readFileSync(path, "utf8");
  const site = relative(DASHBOARD_ROOT, path);
  const found: Offender[] = [];
  let at = source.indexOf(BUTTON_OPEN);
  while (at !== -1) {
    const after = source[at + BUTTON_OPEN.length];
    // `<ButtonGroup` and friends are not the primitive.
    if (after === undefined || /[A-Za-z]/.test(after)) {
      at = source.indexOf(BUTTON_OPEN, at + 1);
      continue;
    }
    const end = tagEnd(source, at + BUTTON_OPEN.length);
    const tag = source.slice(at, end + 1);
    const line = source.slice(0, at).split("\n").length;
    const literal = /variant="([a-z-]+)"/.exec(tag);
    const dynamic = /variant=\{/.test(tag);
    if (literal && !PERMITTED_LITERAL_VARIANTS.has(literal[1]!)) {
      found.push({ site: `${site}:${line}`, variant: literal[1]! });
    } else if (dynamic && !CONFIG_DRIVEN_SITES.has(site)) {
      found.push({ site: `${site}:${line}`, variant: "variant={…} outside the config-driven allowlist" });
    }
    at = source.indexOf(BUTTON_OPEN, end);
  }
  return found;
}

describe("dashboard action buttons", () => {
  it("no dashboard action button uses a variant outside the rule", () => {
    const offenders = tsxFiles(DASHBOARD_ROOT).flatMap(offendersIn);
    expect(
      offenders.map((o) => `${o.site} ${o.variant}`),
      "every action button is default (by omission), ghost, destructive, or link",
    ).toEqual([]);
  });
});
