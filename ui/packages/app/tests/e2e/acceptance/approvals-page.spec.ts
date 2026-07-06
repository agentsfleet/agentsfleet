/**
 * approvals-page.spec.ts — /approvals renders for an authenticated user.
 *
 * Real approval gates are emitted by the runner when an HITL-typed tool
 * call requests human review; reproducing that end-to-end requires a
 * worker pool turn the current fixture harness deliberately scopes out
 * (same heavyweight dependency `events.spec.ts` carries today). The
 * pragmatic acceptance here matches the events spec's posture: the page
 * loads,
 * SSR resolves the active workspace, and the "Pending" section renders.
 * A regression that breaks server-side token resolution lands as a
 * /sign-in redirect; that's the primary signal.
 *
 * Empty-state copy is rendered by ApprovalsList when initial.items is
 * empty. The fixture user starts with no approval gates so the
 * empty-state branch is the expected render.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { gotoWorkspace, workspaceUrlPattern } from "./fixtures/nav";

test.describe("approvals page", () => {
  test("workspace approvals page renders authenticated content", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await gotoWorkspace(page, FIXTURE_KEY.regular, "approvals");
    await expect(page).toHaveURL(workspaceUrlPattern("approvals"));

    await expect(page.getByRole("heading", { name: /^approvals$/i })).toBeVisible();
    await expect(page.getByLabel("Pending approval gates")).toBeVisible();
  });
});
