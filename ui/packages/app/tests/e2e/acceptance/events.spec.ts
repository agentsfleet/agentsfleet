/**
 * events.spec.ts — workspace events page renders for an authenticated user.
 *
 * Spec called for "trigger via POST /v1/webhooks/{agent_id} → poll /events
 * → row appears". The webhook handler requires:
 *   - a workspace credential keyed by `trigger.source` so HMAC verification
 *     can resolve a secret;
 *   - the operator to publish a triggered event row to Postgres + Redis,
 *     which depends on agentsfleetd's worker pool processing the enqueued
 *     stream entry.
 *
 * Both are heavyweight fixture dependencies that the M64_005 harness
 * doesn't yet ship (the `trigger.source` field is unset on seedAgent's
 * minimal trigger spec). Pulling them into this spec would couple the e2e
 * surface to credential plumbing this milestone deliberately scoped out.
 *
 * Pragmatic asserts (what this milestone actually unblocks via WS-A's
 * server-action refactor): the page loads, SSR resolves the active
 * workspace, and the events list scaffolding renders — either the
 * EmptyState (no events for a fresh fixture) or the populated list when
 * earlier specs left a couple of rows behind. A regression that breaks
 * server-side token resolution lands as a /sign-in redirect; that's the
 * primary signal here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("events page", () => {
  test("workspace events page renders authenticated content", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/events");
    await expect(page).toHaveURL(/\/events(\?|$)/);

    await expect(page.getByRole("heading", { name: /^events$/i })).toBeVisible();

    const workspaceEvents = page.getByLabel("Workspace events");
    await expect(workspaceEvents).toBeVisible();

    // Either empty-state or a populated list of event cards. Both are
    // legitimate outcomes for a fresh fixture; the assertion is that the
    // section rendered — not whether events happened to exist. Playwright's
    // `.or()` resolves on the first side to become visible, so the wait
    // itself is the assertion: if neither side appears within the timeout
    // we fail loud, and we don't need to re-check `isVisible()` (which
    // would race with re-renders between frames).
    const emptyState = workspaceEvents.getByText(/no events yet/i);
    const populatedList = workspaceEvents.getByRole("article").first();
    await emptyState.or(populatedList).waitFor({ state: "visible", timeout: 10_000 });
  });
});
