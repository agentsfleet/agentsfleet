/**
 * kill.spec.ts — operator kills a running zombie via the dashboard.
 *
 * Wire: API-seed → /zombies/[id] → KillSwitch "Kill" → ConfirmDialog
 * confirm → return to /zombies and assert the row's `data-state` is
 * `failed` (the dashboard's translation of zombied's `killed` status,
 * per `liveStateOf` in
 * `app/(dashboard)/zombies/components/ZombiesList.tsx:19`).
 *
 * Sister to lifecycle.spec.ts; both exercise the same KillSwitch +
 * ConfirmDialog wiring but with different target statuses. Killing is
 * terminal — the detail page's action panel collapses to a disabled
 * "Killed" indicator afterwards (no Resume / Stop / Kill options).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("kill", () => {
  // FIXME: blocked on client-side Clerk session in e2e (see
  // `lifecycle.spec.ts` describe-block FIXME — same root cause).
  test.fixme("Kill transitions the row's data-state from live to failed (terminal)", async ({
    page,
  }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `kill-${tag}`;
    const seeded = await seedZombie(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`/zombies/${seeded.id}`);
    await expect(page).toHaveURL(new RegExp(`/zombies/${seeded.id}(\\?|$)`));

    await page.getByRole("button", { name: "Kill" }).first().click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    const patched = page.waitForResponse(
      (res) =>
        res.url().includes(`/zombies/${seeded.id}`) && res.request().method() === "PATCH",
    );
    await dialog.getByRole("button", { name: "Kill" }).click();
    await patched;

    // Detail page collapses to the terminal "Killed" indicator once
    // router.refresh() re-runs the SSR with the new status.
    await expect(page.getByRole("button", { name: "Killed" })).toBeDisabled();

    // Dashboard listing: row still appears (list.zig does not filter
    // killed rows) but state dot is `failed`.
    await page.goto("/zombies");
    const row = page.locator(`a[href="/zombies/${seeded.id}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", "failed");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
