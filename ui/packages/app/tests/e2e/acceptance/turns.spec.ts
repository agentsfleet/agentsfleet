/**
 * turns.spec.ts — workspace turns page renders for an authenticated user.
 *
 * Spec called for "trigger via POST /v1/webhooks/{fleet_id}/{source} → poll /turns
 * → row appears". The webhook handler requires:
 *   - a workspace credential keyed by `trigger.source` so HMAC verification
 *     can resolve a secret;
 *   - the daemon to admit the delivery to the Postgres turns table,
 *     which a runner then runs.
 *
 * Both are heavyweight fixture dependencies that the M64_005 harness
 * doesn't yet ship (the `trigger.source` field is unset on seedFleet's
 * minimal trigger spec). Pulling them into this spec would couple the e2e
 * surface to credential plumbing this milestone deliberately scoped out.
 *
 * Pragmatic asserts (what this milestone actually unblocks via WS-A's
 * server-action refactor): the page loads, SSR resolves the active
 * workspace, and the turns list scaffolding renders — either the
 * EmptyState (no turns for a fresh fixture) or the populated list when
 * earlier specs left a couple of rows behind. A regression that breaks
 * server-side token resolution lands as a /sign-in redirect; that's the
 * primary signal here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { gotoWorkspace, workspaceUrlPattern } from "./fixtures/nav";

test.describe("turns page", () => {
  test("workspace turns page renders authenticated content", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "turns");
    await expect(page).toHaveURL(workspaceUrlPattern("turns"));

    await expect(page.getByRole("heading", { name: "Workspace activity", exact: true })).toBeVisible();

    // Exact: once rows exist the turns table adds its own scroll region,
    // named "Workspace activity, scrollable", inside this landmark. A substring
    // match resolves to both and fails strict mode on any populated fixture.
    const workspaceTurns = page.getByLabel("Workspace activity", { exact: true });
    await expect(workspaceTurns).toBeVisible();

    // Either empty-state or a populated list of turn cards. Both are
    // legitimate outcomes for a fresh fixture; the assertion is that the
    // section rendered — not whether turns happened to exist. Playwright's
    // `.or()` resolves on the first side to become visible, so the wait
    // itself is the assertion: if neither side appears within the timeout
    // we fail loud, and we don't need to re-check `isVisible()` (which
    // would race with re-renders between frames).
    const emptyState = workspaceTurns.getByText(/no activity yet/i);
    const populatedList = workspaceTurns.getByRole("button", { name: /inspect turn/i }).first();
    await emptyState.or(populatedList).waitFor({ state: "visible", timeout: 10_000 });
  });
});
