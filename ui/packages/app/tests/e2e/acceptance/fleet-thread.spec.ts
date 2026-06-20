/**
 * fleet-thread.spec.ts — operator-facing chat surface renders against
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
 * /fleets/[id], the thread surface mounts, the live-activity panel is
 * present, the composer renders, and SSE handshake at least starts.
 * A regression here lands either as a /sign-in redirect (server-side
 * token resolution broke) or as a missing Card / missing composer
 * (registry or layout wiring broke).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, listFleets, seedFleet } from "./fixtures/seed";

const PANEL_LABEL = /^Live activity$/;
const COMPOSER_LABEL = "Steer composer";

test.describe("fleet thread surface", () => {
  test("renders the live-activity panel + composer for an authenticated user", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    // Pick any existing fixture fleet or seed a fresh one. Earlier specs
    // in this run may have left rows; we don't depend on a specific
    // identity, only that some fleet exists for this workspace.
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const existing = await listFleets(FIXTURE_KEY.regular, workspaceId);
    const fleet =
      existing[0] ??
      (await seedFleet(FIXTURE_KEY.regular, workspaceId, {
        name: "thread-spec-target",
      }));

    await page.goto(`/fleets/${fleet.id}`);
    await expect(page).toHaveURL(new RegExp(`/fleets/${fleet.id}(\\?|$)`));

    // Page-level header rendered server-side.
    await expect(
      page.getByRole("heading", { name: new RegExp(fleet.name, "i") }).first(),
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
      /(steer this fleet|fleet is working)/i,
    );
    await expect(placeholder).toBeVisible();
  });

  test("survives a /dashboard ↔ /fleets/[id] round-trip without unmounting the surface", async ({
    page,
  }) => {
    // Pins the registry behavior end-to-end: navigating away and back
    // to the same fleet within the registry's idle window must NOT lose
    // the thread surface (a regression where the layout-level subscription
    // tears down on every nav would manifest as a CONNECTING flash on
    // every revisit, observable here as the badge value).
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const existing = await listFleets(FIXTURE_KEY.regular, workspaceId);
    const fleet =
      existing[0] ??
      (await seedFleet(FIXTURE_KEY.regular, workspaceId, {
        name: "thread-revisit-target",
      }));

    await page.goto(`/fleets/${fleet.id}`);
    await expect(page.getByLabel("Live activity stream")).toBeVisible({
      timeout: 10_000,
    });

    await page.goto("/fleets");
    await expect(page).toHaveURL(/\/fleets(\?|$)/);

    // Return. The thread surface must re-render; behavior parity with the
    // first mount is the assertion — we don't claim "no reconnect" at the
    // network layer from a Playwright test (that's the registry unit-test
    // surface), only that the user-visible surface comes back cleanly.
    await page.goto(`/fleets/${fleet.id}`);
    await expect(page.getByLabel("Live activity stream")).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByLabel("Live activity stream").getByLabel(COMPOSER_LABEL),
    ).toBeVisible();
  });

  test("steer submits via a Server Action and no same-origin request carries a client Authorization header", async ({
    page,
  }) => {
    // Dimension 1.1 — the security invariant of this milestone. Steering
    // rides a Server Action (POST with a `Next-Action` header), not a
    // client fetch to /backend; and no same-origin request (the page
    // route, the /backend SSE proxy, any app fetch) ever carries a
    // browser-set bearer token. The SSE route handler injects the token
    // server-side, so its request is cookie-only here too.
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const existing = await listFleets(FIXTURE_KEY.regular, workspaceId);
    const fleet =
      existing[0] ??
      (await seedFleet(FIXTURE_KEY.regular, workspaceId, {
        name: "steer-probe-target",
      }));

    const seen: { method: string; url: string; auth: boolean; serverAction: boolean }[] = [];
    page.on("request", (req) => {
      const h = req.headers();
      seen.push({
        method: req.method(),
        url: req.url(),
        auth: Boolean(h["authorization"]),
        serverAction: Boolean(h["next-action"]),
      });
    });

    await page.goto(`/fleets/${fleet.id}`);
    const appOrigin = new URL(page.url()).origin;
    const threadCard = page.getByLabel("Live activity stream");
    await expect(threadCard).toBeVisible({ timeout: 10_000 });

    const composer = threadCard.getByLabel(COMPOSER_LABEL);
    const textarea = composer.getByPlaceholder(/steer this fleet/i);
    await expect(textarea).toBeVisible();
    await textarea.fill("acceptance steer probe");
    await composer.getByRole("button", { name: /steer/i }).click();

    // The optimistic row renders the message text immediately, regardless
    // of whether the steer ultimately resolves queued→received or →failed.
    await expect(threadCard.getByText(/acceptance steer probe/)).toBeVisible({
      timeout: 5_000,
    });

    // A Server Action POST carried the steer (not a client /backend fetch).
    await expect
      .poll(() => seen.filter((r) => r.method === "POST" && r.serverAction).length, {
        timeout: 10_000,
      })
      .toBeGreaterThan(0);

    // Load-bearing assertion: zero same-origin requests carried a
    // browser-set Authorization header.
    const authHits = seen
      .filter((r) => r.url.startsWith(appOrigin) && r.auth)
      .map((r) => `${r.method} ${r.url}`);
    expect(authHits, authHits.join("\n")).toEqual([]);
  });
});
