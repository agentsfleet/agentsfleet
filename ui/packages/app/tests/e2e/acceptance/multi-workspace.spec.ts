/**
 * multi-workspace.spec.ts — WorkspaceSwitcher dropdown round-trip.
 *
 * Ensures the fixture user has at least two workspaces, then exercises the
 * header WorkspaceSwitcher: open the menu, pick the non-active workspace, and
 * assert the active label updates AND the URL navigates from the primary
 * workspace to the secondary. Post-M118 the workspace lives in the URL
 * (`/w/<id>/…`), so picking a workspace is a `router.push` navigation — the
 * path segment swaps from `/w/<primary>/fleets` to `/w/<secondary>/fleets`,
 * preserving the current sub-path — not a cookie write + revalidate.
 *
 * Spec calls for the `admin` fixture (memberships in both fixture tenants)
 * but the M64_005 harness only provisions one tenant per Clerk user, so we
 * pragmatically seed a second workspace inside the regular fixture's tenant
 * via POST /v1/workspaces. Same UI surface — same WorkspaceSwitcher render
 * path, same navigation — without depending on cross-tenant membership wiring
 * that the harness doesn't yet ship.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { ensureSecondWorkspace, getDefaultWorkspaceId } from "./fixtures/seed";
import { gotoWorkspace, workspaceUrlPattern } from "./fixtures/nav";
import { FIXTURE_KEY, SECOND_WORKSPACE_NAME } from "./fixtures/constants";

const SWITCH_TIMEOUT_MS = 10_000;

test.describe("multi-workspace switcher", () => {
  test("switcher navigates from the primary workspace URL to the secondary", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);
    const landed = await gotoWorkspace(page, FIXTURE_KEY.regular, "fleets");
    expect(landed).toEqual(primary);
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

    // The switcher's visible text is the *active* workspace name (e.g.
    // "default", "fixture-secondary"), so a getByRole({ name: ... }) match
    // would shift on every workspace switch. data-testid is the structural
    // handle that's stable across renders.
    const switcher = page.getByTestId("workspace-switcher");
    await expect(switcher).toBeVisible();
    const initialLabel = (await switcher.textContent())?.trim();
    expect(initialLabel?.length ?? 0).toBeGreaterThan(0);

    await switcher.click();
    await page.getByRole("menuitem", { name: secondary.name ?? secondary.id }).click();

    // Picking the secondary workspace is a `router.push` that swaps the
    // workspace segment while preserving the sub-path: /w/<primary>/fleets →
    // /w/<secondary>/fleets. The listing re-fetches against the new workspace.
    await expect(page).toHaveURL(new RegExp(`/w/${secondary.id}/fleets(\\?|$)`), {
      timeout: SWITCH_TIMEOUT_MS,
    });
    await expect(switcher).toContainText(secondary.name ?? secondary.id, {
      timeout: SWITCH_TIMEOUT_MS,
    });
  });

  // No fleet teardown: this spec seeds no fleets — it only switches
  // workspaces. The unscoped sweep it used to run here deleted sibling
  // workers' fleets mid-test the moment the suite went parallel; leftover
  // rows from interrupted runs are the global-setup janitor's job.
});
