import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * A table that paginates fills the height its page gives it.
 *
 * Two facts have to agree for that to happen, and for a long time only one of
 * them was checked by anything. `flex-1` fills a parent of DEFINITE height, so
 * the page shell must be `h-full` — a `min-h-full` is a minimum, not a height,
 * and a table under it collapses to its own content. Then the table itself has
 * to opt in: `DataTable` is `max-h-[…]` by default, so a caller that does not
 * pass the fill classes stops short no matter what its shell says.
 *
 * Both halves shipped broken, independently, on two different surfaces:
 * Events had the table wiring and a `min-h-full` shell, and Platform > Fleet
 * library had the shell and no table wiring. Each looked correct in isolation
 * and neither spanned, while Secrets — which had both — did. A test that
 * checked one half would have passed on both bugs.
 */

const DASHBOARD_ROOT = join(__dirname, "..", "app", "(dashboard)");
const TEST_FILE_MARK = ".test.";

/** The shell a page holding a filling table must open with. */
const FULL_HEIGHT_SHELL = /<PageLayout\b[^>]*\bclassName="[^"]*\bh-full\b/;

/** What a caller passes to make `DataTable` fill rather than cap its height. */
const TABLE_FILLS = /viewportClassName="[^"]*\bflex-1\b/;

/**
 * Surfaces whose list is deliberately NOT a filling table.
 *
 * `approvals` renders a card list rather than a `DataTable`, and the settings
 * pages are forms. Neither has a viewport to fill, so neither is measured.
 */
const NOT_A_FILLING_TABLE: ReadonlySet<string> = new Set([
  "w/[workspaceId]/approvals/page.tsx",
  "w/[workspaceId]/approvals/[gateId]/page.tsx",
  "w/[workspaceId]/fleets/new/page.tsx",
  "w/[workspaceId]/integrations/page.tsx",
  "w/[workspaceId]/settings/defaults/page.tsx",
  "w/[workspaceId]/settings/security/page.tsx",
  "settings/page.tsx",
  "settings/account/[[...account]]/page.tsx",
  "admin/fleet-libraries/page.tsx",
  "w/[workspaceId]/fleets/page.tsx",
]);

function sourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    const isSource = name.endsWith(".tsx") && !name.includes(TEST_FILE_MARK);
    return isSource ? [path] : [];
  });
}

describe("a paginating table fills the page it sits on", () => {
  it("gives every filling table's page a definite-height shell", () => {
    // A page whose table asks to fill, under a shell that never gave it a
    // height to fill. This is the Events bug, stated as a rule.
    const offenders = sourceFiles(DASHBOARD_ROOT)
      .filter((path) => path.endsWith("page.tsx"))
      .filter((path) => !NOT_A_FILLING_TABLE.has(relative(DASHBOARD_ROOT, path)))
      .filter((path) => {
        const source = readFileSync(path, "utf8");
        return source.includes("<PageLayout") && !FULL_HEIGHT_SHELL.test(source);
      })
      .map((path) => relative(DASHBOARD_ROOT, path));

    expect(offenders).toEqual([]);
  });

  it("makes both Fleet library tables fill, so the two surfaces match", () => {
    // The pair Indy put side by side: same data, same DataTable, one spanned
    // and one did not. Named explicitly rather than swept, because "the two
    // Fleet libraries look the same" is the actual requirement.
    for (const table of [
      "w/[workspaceId]/library/components/WorkspaceLibraryList.tsx",
      "admin/fleet-libraries/components/PlatformCatalogTable.tsx",
    ]) {
      const source = readFileSync(join(DASHBOARD_ROOT, table), "utf8");
      expect(TABLE_FILLS.test(source), `${table} caps its height instead of filling`).toBe(true);
      expect(source).toContain("flex min-h-0 flex-1 flex-col");
    }
  });

  it("keeps the Events page on the shell its table needs", () => {
    // The regression this file was written for. `min-h-full` is the spelling
    // that broke it, and it reads almost identically to the correct one.
    const source = readFileSync(join(DASHBOARD_ROOT, "w/[workspaceId]/events/page.tsx"), "utf8");
    expect(FULL_HEIGHT_SHELL.test(source)).toBe(true);
    expect(source).not.toMatch(/<PageLayout\b[^>]*\bmin-h-full\b/);
  });
});
