/**
 * workspace-url-flow.spec.ts — M118 headline flow: the workspace is an explicit
 * URL segment (`/w/<id>/…`), not an implicit cookie/claim.
 *
 * Drives the four behaviours the refactor introduces:
 *   1. Land — `/` redirects once to the first owned workspace's URL (`/w/<id>`).
 *   2. Deep-link — visiting an owned workspace's Integrations page loads its
 *      connectors (this is the original "Couldn't load connectors" bug: the
 *      implicit active-workspace guess is gone, so the page keys off the URL id
 *      the backend re-authorizes with `ownsWithinTenant`).
 *   3. Switch — picking another workspace changes the URL (`/w/<other>/…`) and
 *      reloads the data; it is a navigation, not a cookie write.
 *
 * Read-only on tenant state (only ensures a second workspace exists, idempotent).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, ensureSecondWorkspace } from "./fixtures/seed";
import { gotoWorkspace, workspaceUrlPattern, workspaceHref } from "./fixtures/nav";

const SECOND_WORKSPACE_NAME = "fixture-secondary";
const SWITCH_TIMEOUT_MS = 10_000;

test.describe("workspace in the URL", () => {
  test("landing on / redirects to the first owned workspace's URL", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await signInAs(page, FIXTURE_KEY.regular);

    await page.goto("/");
    // The entry redirect lands on `/w/<first-owned>/…`, never a bare `/`.
    await expect(page).toHaveURL(new RegExp(`/w/${primary}(/|$|\\?)`), {
      timeout: SWITCH_TIMEOUT_MS,
    });
    await expect(page.getByRole("heading", { name: /^dashboard$/i })).toBeVisible();
  });

  test("deep-linking an owned workspace's Integrations loads its connectors", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "integrations");

    await expect(page).toHaveURL(workspaceUrlPattern("integrations"));
    await expect(page.getByRole("heading", { name: /^integrations$/i })).toBeVisible();
    // The original bug: the connectors region failed to load ("Couldn't load
    // connectors"). It must render, with no load-failure banner.
    await expect(page.getByTestId("integrations-page")).toBeVisible();
    await expect(page.getByText(/couldn'?t load connectors/i)).toHaveCount(0);
  });

  test("switching workspace changes the URL and preserves the sub-page", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
    await expect(page).toHaveURL(new RegExp(`/w/${primary}/fleets`));

    const switcher = page.getByTestId("workspace-switcher");
    await expect(switcher).toBeVisible();
    await switcher.click();
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();

    // Selection is a navigation: the URL moves to the secondary workspace on the
    // same sub-page (fleets), and the switcher label follows.
    await expect(page).toHaveURL(new RegExp(`/w/${secondary.id}/fleets`), {
      timeout: SWITCH_TIMEOUT_MS,
    });
    await expect(switcher).toContainText(secondary.name ?? secondary.id, {
      timeout: SWITCH_TIMEOUT_MS,
    });
  });

  test("a workspace deep-link is stable and shareable", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await signInAs(page, FIXTURE_KEY.regular);

    // Navigating straight to a workspace's Events page (as a bookmark/share would)
    // lands on exactly that page — no re-resolution, no bounce.
    await page.goto(workspaceHref(primary, "events"));
    await expect(page).toHaveURL(new RegExp(`/w/${primary}/events`));
    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();
  });
});
