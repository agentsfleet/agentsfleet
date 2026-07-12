/**
 * platform-library-onboarding.spec.ts — a platform operator fills the fleet
 * catalog from the dashboard, and nobody else can.
 *
 * This is the surface that turns a seeded-but-empty catalog into an installable
 * one: the seed rows carry curated metadata but no bundle, and the workspace
 * gallery hides a row until its `content_hash` lands. Onboarding is the only
 * thing that lands it, and until now onboarding had no UI at all.
 *
 * Three claims, each of which a unit test cannot make:
 *   - the scope actually gates the route end-to-end (real Clerk session, real
 *     agentsfleetd `requireScope`), not just the mocked `hasScope`;
 *   - a bad repository surfaces the importer's real UZ error in the dialog;
 *   - a real GitHub import lands a real row that a real workspace can see.
 *
 * The operator fixture is the only one whose Clerk `public_metadata.scopes`
 * carries `platform-library:write` (fixtures/constants.ts). The regular fixture
 * is deliberately scope-free, which is what makes the negative case meaningful.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY } from "./fixtures/constants";

const ADMIN_PATH = "/admin/fleet-libraries";
const NAV_LABEL = "Fleet libraries";

// Seeded in schema/023_fleet_library.sql and published at agentsfleet/<id>. The
// onboard upserts onto the seeded row, so the catalog id is the bundle's
// SKILL.md frontmatter name — not the repository path the operator types.
const SAMPLE_REPO = "agentsfleet/platform-ops";
const SAMPLE_ENTRY_ID = "platform-ops";

// A repository that does not exist, to drive the importer's fetch failure into
// the dialog rather than a crash or a silent close.
const MISSING_REPO = "agentsfleet/definitely-not-a-fleet-bundle";

test.describe("platform fleet-library onboarding", () => {
  test("a workspace user never sees the operator surface", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    await page.goto("/");
    await expect(page.getByRole("link", { name: NAV_LABEL })).toHaveCount(0);

    // Typing the URL directly is refused too — the nav is discoverability, the
    // page guard is the redirect, and agentsfleetd would 403 regardless.
    await page.goto(ADMIN_PATH);
    await expect(page).not.toHaveURL(new RegExp(`${ADMIN_PATH}$`));
  });

  test("an operator reaches the surface from the nav", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);

    await page.goto("/");
    await page.getByRole("link", { name: NAV_LABEL }).click();

    await expect(page).toHaveURL(new RegExp(`${ADMIN_PATH}$`));
    await expect(page.getByRole("heading", { name: NAV_LABEL })).toBeVisible();
  });

  test("a repository that cannot be imported keeps the dialog open with the error", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);

    await page.getByRole("button", { name: /onboard fleet/i }).click();
    await page.getByLabel(/repository/i).fill(MISSING_REPO);
    await page.getByRole("button", { name: /^onboard$/i }).click();

    // The dialog stays mounted with the failure shown — the operator corrects
    // the repository in place rather than losing what they typed.
    await expect(page.getByRole("alert")).toBeVisible({ timeout: 30_000 });
    await expect(page.getByLabel(/repository/i)).toBeVisible();
  });

  test("onboarding lands the entry in the platform catalog", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.operator);
    await page.goto(ADMIN_PATH);

    await page.getByRole("button", { name: /onboard fleet/i }).click();
    await page.getByLabel(/repository/i).fill(SAMPLE_REPO);
    await page.getByRole("button", { name: /^onboard$/i }).click();

    // The dialog closes and the entry the SERVER returned renders — the id is
    // the bundle's declared name, which proves the importer, the object-store
    // write, and the catalog upsert all ran.
    const entry = page.getByTestId(`onboarded-entry-${SAMPLE_ENTRY_ID}`);
    await expect(entry).toBeVisible({ timeout: 60_000 });
    await expect(entry).toContainText(SAMPLE_ENTRY_ID);
    await expect(entry).toContainText("platform");
  });

  test("the onboarded fleet becomes installable in a workspace gallery, exactly once", async ({
    page,
  }) => {
    // Onboard the same repository TWICE. The catalog id comes from the bundle's
    // declared name, not the request, so the second onboard must upsert the
    // first row rather than mint a second entry.
    await signInAs(page, FIXTURE_KEY.operator);
    for (const _attempt of [1, 2]) {
      await page.goto(ADMIN_PATH);
      await page.getByRole("button", { name: /onboard fleet/i }).click();
      await page.getByLabel(/repository/i).fill(SAMPLE_REPO);
      await page.getByRole("button", { name: /^onboard$/i }).click();
      await expect(page.getByTestId(`onboarded-entry-${SAMPLE_ENTRY_ID}`)).toBeVisible({
        timeout: 60_000,
      });
    }

    // A plain workspace user, who can reach no operator surface at all, can now
    // install it — the whole point of the platform tier. The gallery is also
    // the only place a duplicate catalog row would be visible: the admin page
    // renders one card by construction, so asserting there proves nothing.
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/");
    await expect(page).toHaveURL(workspaceUrlPattern());

    const workspacePath = new URL(page.url()).pathname;
    await page.goto(`${workspacePath}/fleets/new`);

    const cards = page.getByTestId(`library-card-${SAMPLE_ENTRY_ID}`);
    await expect(cards.first()).toBeVisible({ timeout: 30_000 });
    await expect(cards).toHaveCount(1);
  });
});
