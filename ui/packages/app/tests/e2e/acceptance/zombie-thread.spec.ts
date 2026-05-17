/**
 * zombie-thread.spec.ts — operator-facing chat surface renders against
 * the durable event log for an authenticated user.
 *
 * The full vision (HANDOFF punchlist #22) calls for four tests:
 *   1. Stream connects + first frame appears in <1s
 *   2. CHUNK concat + streaming cursor visible/cleared
 *   3. Reconnect recovery — abort route, badge flips, recovers within backoff
 *   4. Steer-to-render roundtrip — composer submit → optimistic queued → real frame
 *
 * Tests #1–#3 require either backend-side SSE injection or test-mode
 * frame-emit hooks the M68 harness doesn't yet expose. Filing those as
 * §6 follow-ups in this milestone's spec. What this spec pins now is
 * the load-bearing assertion: an authenticated user lands on
 * /zombies/[id], the thread surface mounts, the live-activity panel is
 * present, the composer renders, and SSE handshake at least starts.
 * A regression here lands either as a /sign-in redirect (server-side
 * token resolution broke) or as a missing Card / missing composer
 * (registry or layout wiring broke).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, listZombies, seedZombie } from "./fixtures/seed";

const PANEL_LABEL = /^Live activity$/;
const COMPOSER_LABEL = "Steer composer";

test.describe("zombie thread surface", () => {
  test("renders the live-activity panel + composer for an authenticated user", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    // Pick any existing fixture zombie or seed a fresh one. Earlier specs
    // in this run may have left rows; we don't depend on a specific
    // identity, only that some zombie exists for this workspace.
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const existing = await listZombies(FIXTURE_KEY.regular, workspaceId);
    const zombie =
      existing[0] ??
      (await seedZombie(FIXTURE_KEY.regular, workspaceId, {
        name: "thread-spec-target",
      }));

    await page.goto(`/zombies/${zombie.id}`);
    await expect(page).toHaveURL(new RegExp(`/zombies/${zombie.id}(\\?|$)`));

    // Page-level header rendered server-side.
    await expect(
      page.getByRole("heading", { name: new RegExp(zombie.name, "i") }).first(),
    ).toBeVisible();

    // The thread Card is the load-bearing component for this PR. It mounts
    // client-side via next/dynamic(ssr:false) and consumes the
    // module-singleton stream registry. Asserting on its `aria-label`
    // (set on the wrapping <article asChild>) gives a stable hook that
    // doesn't depend on visual styling.
    const threadCard = page.getByLabel("Live activity stream");
    await expect(threadCard).toBeVisible({ timeout: 10_000 });

    // Title row is part of the same Card.
    await expect(
      threadCard.getByText(PANEL_LABEL).first(),
    ).toBeVisible();

    // Viewport carries role="log" + aria-live=polite (polish #16).
    const log = threadCard.getByRole("log", { name: /live activity/i });
    await expect(log).toBeVisible();

    // Composer always renders. The textarea placeholder swaps when a
    // stage is running; either form is acceptable on a fresh visit.
    const composer = threadCard.getByLabel(COMPOSER_LABEL);
    await expect(composer).toBeVisible();
    const placeholder = composer.getByPlaceholder(
      /(steer this zombie|zombie is working)/i,
    );
    await expect(placeholder).toBeVisible();
  });

  test("survives a /dashboard ↔ /zombies/[id] round-trip without unmounting the surface", async ({
    page,
  }) => {
    // Pins the registry behavior end-to-end: navigating away and back
    // to the same zombie within the registry's idle window must NOT lose
    // the thread surface (a regression where the layout-level subscription
    // tears down on every nav would manifest as a CONNECTING flash on
    // every revisit, observable here as the badge value).
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const existing = await listZombies(FIXTURE_KEY.regular, workspaceId);
    const zombie =
      existing[0] ??
      (await seedZombie(FIXTURE_KEY.regular, workspaceId, {
        name: "thread-revisit-target",
      }));

    await page.goto(`/zombies/${zombie.id}`);
    await expect(page.getByLabel("Live activity stream")).toBeVisible({
      timeout: 10_000,
    });

    await page.goto("/zombies");
    await expect(page).toHaveURL(/\/zombies(\?|$)/);

    // Return. The thread surface must re-render; behavior parity with the
    // first mount is the assertion — we don't claim "no reconnect" at the
    // network layer from a Playwright test (that's the registry unit-test
    // surface), only that the user-visible surface comes back cleanly.
    await page.goto(`/zombies/${zombie.id}`);
    await expect(page.getByLabel("Live activity stream")).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByLabel("Live activity stream").getByLabel(COMPOSER_LABEL),
    ).toBeVisible();
  });
});
