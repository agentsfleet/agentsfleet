/**
 * settings-provider.spec.ts — /settings/provider renders for an authed user.
 *
 * Asserts the page resolves the active workspace, calls
 * GET /v1/tenants/me/provider (resolved server-side), and renders the
 * "Active configuration" + "Change provider" sections. Does NOT switch
 * the provider mode — that touches tenant state and would race other
 * specs running against the same fixture tenant; the change-flow is
 * covered by unit tests under `lib/api/tenant_provider.test.ts`.
 *
 * Page render alone is a useful signal because the provider resolver
 * has multiple failure modes (synthesised default, credential ref
 * mismatch, backend 5xx) that all surface as visible chrome here.
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("settings provider page", () => {
  test("LLM provider settings render for the fixture tenant", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto("/settings/provider");
    await expect(page).toHaveURL(/\/settings\/provider(\?|$)/);

    await expect(page.getByRole("heading", { name: /^llm provider$/i })).toBeVisible();
    await expect(page.getByLabel("Active provider configuration")).toBeVisible();
    await expect(page.getByLabel("Change provider")).toBeVisible();
  });
});
