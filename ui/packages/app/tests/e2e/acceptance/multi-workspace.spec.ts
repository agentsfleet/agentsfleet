/**
 * multi-workspace.spec.ts — WorkspaceSwitcher dropdown round-trip.
 *
 * Ensures the fixture user has at least two workspaces, then exercises the
 * header WorkspaceSwitcher: open the menu, pick the non-active workspace,
 * and assert the active label updates and the URL stays `/zombies` (the
 * switch is a cookie write + revalidate, not a route change).
 *
 * Spec calls for the `admin` fixture (memberships in both fixture tenants)
 * but the M64_005 harness only provisions one tenant per Clerk user, so we
 * pragmatically seed a second workspace inside the regular fixture's tenant
 * via POST /v1/workspaces. Same UI surface — same WorkspaceSwitcher render
 * path, same `setActiveWorkspace` server action — without depending on
 * cross-tenant membership wiring that the harness doesn't yet ship.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { ensureSecondWorkspace, getDefaultWorkspaceId } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

const SECOND_WORKSPACE_NAME = "fixture-secondary";
const SWITCH_TIMEOUT_MS = 10_000;

test.describe("multi-workspace switcher", () => {
  test("switcher swaps workspace + URL stays /zombies", async ({ page }) => {
    const primary = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const secondary = await ensureSecondWorkspace(FIXTURE_KEY.regular, SECOND_WORKSPACE_NAME);
    expect(secondary.id).not.toEqual(primary);

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/zombies");
    await expect(page).toHaveURL(/\/zombies(\?|$)/);

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

    // The Server Action writes the cookie + revalidatePath('/'); the
    // listing re-fetches but the URL stays /zombies.
    await expect(page).toHaveURL(/\/zombies(\?|$)/, { timeout: SWITCH_TIMEOUT_MS });
    await expect(switcher).toContainText(secondary.name ?? secondary.id, {
      timeout: SWITCH_TIMEOUT_MS,
    });
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
