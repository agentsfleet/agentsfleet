/**
 * fleet-thread.spec.ts — the operator's chat surface renders against the
 * durable event log for an authenticated user.
 *
 * This acceptance surface pins authenticated mount, workspace navigation,
 * the transcript/composer sibling layout, optimistic steer rendering, and
 * the rule that browser requests never carry a client Authorization header.
 * Stream frame handling and reconnect behaviour are covered by the focused
 * registry and event-stream tests where frame timing is deterministic.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { getDefaultWorkspaceId, seedFleet, waitForFleetActive } from "./fixtures/seed";
import { cleanWorkspaceFleets } from "./fixtures/teardown";
import { workspaceHref, workspaceUrlPattern } from "./fixtures/nav";

const PANEL_LABEL = /^Chat$/;
const CHAT_LABEL = "Fleet chat";
const COMPOSER_LABEL = "Chat composer";

// One seed prefix per test, declared beside the name generators so the seeder
// and the afterEach sweep share a single literal and can never drift. The
// sweep is not optional hygiene: every seeded fleet carries a live cron
// trigger, so an unswept row keeps waking runners long after the run ends.
const THREAD_PREFIX = "thread-spec-";
const REVISIT_PREFIX = "thread-revisit-";
const STEER_PROBE_PREFIX = "steer-probe-";
const SEED_PREFIXES = [THREAD_PREFIX, REVISIT_PREFIX, STEER_PROBE_PREFIX] as const;

test.describe("fleet thread surface", () => {
  // Same shape as lifecycle.spec.ts: prefix-scoped so parallel workers
  // sharing the regular fixture workspace never delete a sibling's fleet.
  test.afterEach(async () => {
    const ws = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    for (const prefix of SEED_PREFIXES) {
      await cleanWorkspaceFleets(FIXTURE_KEY.regular, ws, prefix);
    }
  });

  test("renders the chat panel + composer for an authenticated user", async ({
    page,
  }) => {
    await signInAs(page, FIXTURE_KEY.regular);

    // Seed a uniquely-named fleet rather than reusing whatever an earlier
    // spec left behind: sibling specs clean the shared workspace from
    // parallel workers, and a borrowed fleet can vanish mid-test.
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${THREAD_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page).toHaveURL(workspaceUrlPattern(`fleets/${fleet.id}`));

    // Breadcrumb rendered server-side without duplicating the fleet name in a
    // second oversized title row.
    // Scope to the breadcrumb — the sidebar carries its own Fleets link, so
    // the bare role query became ambiguous when the breadcrumb shipped.
    await expect(
      page.getByLabel("Breadcrumb").getByRole("link", { name: "Fleets" }),
    ).toBeVisible();
    await expect(page.getByText(fleet.name, { exact: true }).first()).toBeVisible();

    // The thread card mounts client-side and consumes the shared stream registry.
    // Its accessible label is stable across visual changes.
    const threadCard = page.getByLabel(CHAT_LABEL);
    await expect(threadCard).toBeVisible({ timeout: 10_000 });

    // The chat heading and connection status share one baseline. The removed
    // Steer tab cannot suggest a second view that does not exist.
    await expect(threadCard.getByRole("heading", { name: PANEL_LABEL })).toBeVisible();
    await expect(threadCard.getByRole("link", { name: "Steer" })).toHaveCount(0);
    await expect(threadCard.getByLabel(/Connection status:/i)).toBeVisible();

    // The conversation carries role="log" + aria-live=polite.
    const log = threadCard.getByRole("log", { name: /chat/i });
    await expect(log).toBeVisible();

    // The composer always renders and never disables itself — sending does
    // not depend on the live feed or on the fleet being idle.
    const composer = page.getByLabel(COMPOSER_LABEL);
    await expect(composer).toBeVisible();
    const placeholder = composer.getByPlaceholder(/message this fleet/i);
    await expect(placeholder).toBeVisible();
    await expect(placeholder).toBeEnabled();
  });

  test("survives a /w/[workspaceId]/fleets ↔ /w/[workspaceId]/fleets/[id] round-trip without unmounting the surface", async ({
    page,
  }) => {
    // Pins the registry behavior end-to-end: navigating away and back
    // to the same fleet within the registry's idle window must NOT lose
    // the thread surface (a regression where the layout-level subscription
    // tears down on every nav would manifest as a CONNECTING flash on
    // every revisit, observable here as the badge value).
    await signInAs(page, FIXTURE_KEY.regular);
    const workspaceId = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${REVISIT_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page.getByLabel(CHAT_LABEL)).toBeVisible({
      timeout: 10_000,
    });

    await page.goto(workspaceHref(workspaceId, "fleets"));
    await expect(page).toHaveURL(workspaceUrlPattern("fleets"));

    // Return. The thread surface must re-render; behavior parity with the
    // first mount is the assertion — we don't claim "no reconnect" at the
    // network layer from a Playwright test (that's the registry unit-test
    // surface), only that the user-visible surface comes back cleanly.
    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    await expect(page.getByLabel(CHAT_LABEL)).toBeVisible({
      timeout: 10_000,
    });
    await expect(
      page.getByLabel(COMPOSER_LABEL),
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
    const tag = Math.random().toString(36).slice(2, 8);
    const fleet = await seedFleet(FIXTURE_KEY.regular, workspaceId, {
      name: `${STEER_PROBE_PREFIX}${tag}`,
    });
    await waitForFleetActive(FIXTURE_KEY.regular, workspaceId, fleet.id);

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

    await page.goto(workspaceHref(workspaceId, `fleets/${fleet.id}`));
    const appOrigin = new URL(page.url()).origin;
    const threadCard = page.getByLabel(CHAT_LABEL);
    await expect(threadCard).toBeVisible({ timeout: 10_000 });

    const composer = page.getByLabel(COMPOSER_LABEL);
    const textarea = composer.getByPlaceholder(/message this fleet/i);
    await expect(textarea).toBeVisible();
    await textarea.fill("acceptance steer probe");
    await composer.getByRole("button", { name: /send/i }).click();

    // The optimistic row renders the message text immediately, regardless
    // of whether the send ultimately resolves to sent or to failed.
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
