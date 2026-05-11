/**
 * lifecycle.spec.ts — operator stops a running zombie via the dashboard.
 *
 * Wire: API-seed → /zombies/[id] → KillSwitch "Stop" → ConfirmDialog
 * confirm → return to /zombies and assert the row's `data-state` is
 * `parked` (the dashboard's translation of zombied's `stopped` status,
 * per `liveStateOf` in
 * `app/(dashboard)/zombies/components/ZombiesList.tsx:19`).
 *
 * Sister to kill.spec.ts; both exercise the same KillSwitch + ConfirmDialog
 * wiring but with different target statuses (`stopped` vs `killed`).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { getDefaultWorkspaceId, seedZombie } from "./fixtures/seed";
import { cleanWorkspaceZombies } from "./fixtures/teardown";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("lifecycle", () => {
  // FIXME: blocked on client-side Clerk session in e2e. The Playwright
  // cookie-mount in signInAs() makes clerkMiddleware (SSR) accept the
  // fixture user — listings, detail-page render, and any server-side
  // `getServerToken()` call all work. But the in-browser Clerk SDK
  // initialises by calling FAPI's `/v1/client`; without a real Clerk
  // sign-in the SDK reports signed-out, so `useClientToken().getToken()`
  // returns null and KillSwitch's handleConfirm short-circuits before
  // dispatching the PATCH. Two roads to unblock (whichever lands first):
  //   1. `@clerk/testing` clerk.signIn becomes reliable on this DEV
  //      instance (Captain's open item — tracked in M64_006).
  //   2. Refactor KillSwitch to a server action so the token comes from
  //      `getServerToken()` instead of useAuth. (Bigger scope; defer.)
  // Until then, lifecycle/kill stay fixme — the install-zombie-{seed,cli}
  // specs already prove the seed + dashboard render half of M64_005, and
  // the API contract for stop/resume/kill is covered by zombied's
  // integration tests at src/http/handlers/zombies/*_integration_test.zig.
  test.fixme("Stop transitions the row's data-state from live to parked", async ({ page }) => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const name = `lifecycle-${tag}`;
    const seeded = await seedZombie(FIXTURE_KEY.regular, ws, { name });

    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(`/zombies/${seeded.id}`);
    await expect(page).toHaveURL(new RegExp(`/zombies/${seeded.id}(\\?|$)`));

    // KillSwitch shows Stop + Kill while status is active. Click Stop,
    // then confirm in the ConfirmDialog (a second "Stop" button appears
    // inside the dialog — disambiguate via Radix's role="dialog").
    await page.getByRole("button", { name: "Stop" }).first().click();
    const dialog = page.getByRole("alertdialog");
    await expect(dialog).toBeVisible();
    // Wait on the PATCH /status response — the optimistic UI hides the
    // dialog before the network completes, but the dashboard listing
    // can race the SSR cache otherwise.
    const patched = page.waitForResponse(
      (res) =>
        res.url().includes(`/zombies/${seeded.id}`) && res.request().method() === "PATCH",
    );
    await dialog.getByRole("button", { name: "Stop" }).click();
    await patched;

    // Dashboard listing is the spec's source of truth: a stopped zombie
    // shows as `data-state="parked"` (muted dot per liveStateOf).
    await page.goto("/zombies");
    const row = page.locator(`a[href="/zombies/${seeded.id}"]`);
    await expect(row).toBeVisible();
    await expect(row).toHaveAttribute("data-state", "parked");
  });

  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    await cleanWorkspaceZombies(FIXTURE_KEY.regular, ws);
  });
});
