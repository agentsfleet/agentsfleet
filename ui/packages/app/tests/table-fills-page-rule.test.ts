import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * A table that paginates fills the height its page gives it.
 *
 * Two facts have to agree for that to happen, and for a long time only one of
 * them was checked by anything. `flex-1` fills a parent of DEFINITE height, so
 * the shell must be `h-full` — a `min-h-full` is a minimum, not a height, and
 * a table under it collapses to its own content. Then the table itself has to
 * opt in: `DataTable` caps its height by default, so a caller that does not
 * pass the fill classes stops short no matter what its shell says.
 *
 * Both halves shipped broken, independently, on two different surfaces: Events
 * had the table wiring and a `min-h-full` shell, and Platform > Fleet library
 * had the shell and no table wiring. Each looked correct in isolation and
 * neither spanned, while Secrets — which had both — did. A test that checked
 * one half would have passed on both bugs.
 *
 * ## Why this names its surfaces instead of scanning every page
 *
 * The first shape of this walked every dashboard `page.tsx` and demanded the
 * full-height shell unless the path sat in a `NOT_A_FILLING_TABLE` denylist.
 * Greptile caught it (PR #709): a denylist puts the maintenance burden on
 * pages that have nothing to do with this rule. Ship a settings form and CI
 * goes red until someone reads this file and adds a line — a test failing on
 * code it does not govern, which teaches people to edit the test to make it
 * quiet. That is the opposite of what a rule test is for.
 *
 * So the list is an allowlist, and it is the point rather than an exemption:
 * these are the surfaces where a table must fill. Adding one is a deliberate
 * line; adding a form page is no line at all.
 */

const DASHBOARD_ROOT = join(__dirname, "..", "app", "(dashboard)");

/** The shell a filling table's page must open with. */
const FULL_HEIGHT_SHELL = /<PageLayout\b[^>]*\bclassName="[^"]*\bh-full\b/;

/** What a caller passes to make `DataTable` fill rather than cap its height. */
const TABLE_FILLS = /viewportClassName="[^"]*\bflex-1\b/;

/** The column a flex child needs from its parent to have room to grow into. */
const FLEX_COLUMN = "flex min-h-0 flex-1 flex-col";

/**
 * Every surface whose table must span its page, and the two files that decide
 * it. The shell is not always `page.tsx`: the platform catalogue renders its
 * own `PageLayout` from a client view, because the page component keeps a
 * plain layout for the read-failure branch.
 */
const FILLING_TABLE_SURFACES: ReadonlyArray<{
  readonly surface: string;
  readonly shell: string;
  readonly table: string;
}> = [
  {
    surface: "Configuration > Fleet library",
    shell: "w/[workspaceId]/library/page.tsx",
    table: "w/[workspaceId]/library/components/WorkspaceLibraryList.tsx",
  },
  {
    surface: "Platform > Fleet library",
    shell: "admin/fleet-libraries/components/FleetLibrariesView.tsx",
    table: "admin/fleet-libraries/components/PlatformCatalogTable.tsx",
  },
  {
    surface: "Secrets",
    shell: "w/[workspaceId]/secrets/page.tsx",
    table: "w/[workspaceId]/secrets/components/SecretsList.tsx",
  },
  {
    surface: "Events",
    shell: "w/[workspaceId]/events/page.tsx",
    table: "../../components/domain/EventsList.tsx",
  },
];

function read(relative: string): string {
  return readFileSync(join(DASHBOARD_ROOT, relative), "utf8");
}

describe("a paginating table fills the page it sits on", () => {
  it.each(FILLING_TABLE_SURFACES)(
    "$surface gives its table a definite height to fill",
    ({ shell }) => {
      // The Events half: `min-h-full` reads almost identically to the correct
      // spelling and silently collapses the table under it.
      const source = read(shell);
      expect(FULL_HEIGHT_SHELL.test(source)).toBe(true);
      expect(source).not.toMatch(/<PageLayout\b[^>]*\bmin-h-full\b/);
    },
  );

  it.each(FILLING_TABLE_SURFACES)("$surface asks its table to fill", ({ table }) => {
    // The Platform > Fleet library half: the shell reserved the height and the
    // table never claimed it, so the page looked correct and did not span.
    const source = read(table);
    expect(TABLE_FILLS.test(source), `${table} caps its height instead of filling`).toBe(
      true,
    );
    expect(source).toContain(FLEX_COLUMN);
  });

  it("governs the two Fleet library tables together, so the pair cannot drift", () => {
    // The requirement Indy actually stated, held as its own assertion: same
    // data, same DataTable, the two surfaces look the same.
    const named = FILLING_TABLE_SURFACES.map((entry) => entry.surface);
    expect(named).toContain("Configuration > Fleet library");
    expect(named).toContain("Platform > Fleet library");
  });
});
