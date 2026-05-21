/**
 * settings-api-keys.spec.ts — /settings/api-keys full round-trip for an
 * operator-role tenant user: mint → one-time reveal → close → revoke → delete.
 *
 * Uses FIXTURE_KEY.admin because the route is operator-gated (the backend's
 * operator() middleware 403s a plain user, and the page redirects on 403).
 * The key name is unique per run so the mint/revoke/delete cycle is
 * self-contained and does not race other specs on the shared fixture tenant.
 *
 * Pins the reveal-once invariant: after the dialog closes, the raw zmb_t_
 * value is gone from the DOM (CreateApiKeyDialog discards it).
 */
import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

test.describe("settings api-keys page", () => {
  test("operator round-trip: mint, reveal once, revoke, delete", async ({ page }) => {
    await signInAs(page, FIXTURE_KEY.admin);
    await page.goto("/settings/api-keys");
    await expect(page).toHaveURL(/\/settings\/api-keys(\?|$)/);
    await expect(page.getByRole("heading", { name: /^api keys$/i })).toBeVisible();

    const name = `e2e-${Date.now()}`;

    // Mint
    await page.getByRole("button", { name: /new api key/i }).click();
    await page.getByLabel(/^name$/i).fill(name);
    await page.getByRole("button", { name: /create key/i }).click();

    // Reveal exactly once — the raw value starts with the tenant key prefix.
    const field = page.getByLabel(/api key value/i);
    await expect(field).toBeVisible();
    const raw = await field.inputValue();
    expect(raw.startsWith("zmb_t_")).toBe(true);

    // Close — the raw value must not survive in the DOM (Invariant 1).
    await page.getByRole("button", { name: /stored it/i }).click();
    await expect(page.getByLabel(/api key value/i)).toHaveCount(0);
    await expect(page.getByText(raw, { exact: false })).toHaveCount(0);

    // The new key is listed (newest-first → page 1).
    await expect(page.getByText(name, { exact: true })).toBeVisible();

    // Revoke → the row flips to a revoked state exposing Delete.
    await page.getByRole("button", { name: new RegExp(`revoke api key ${name}`, "i") }).click();
    await page.getByRole("alertdialog").getByRole("button", { name: /^revoke$/i }).click();
    const del = page.getByRole("button", { name: new RegExp(`delete api key ${name}`, "i") });
    await expect(del).toBeVisible();

    // Delete → the row leaves the list.
    await del.click();
    await page.getByRole("alertdialog").getByRole("button", { name: /^delete$/i }).click();
    await expect(page.getByText(name, { exact: true })).toHaveCount(0);
  });
});
