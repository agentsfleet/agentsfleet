import { test, expect } from "@playwright/test";

/**
 * Smoke tests — fast subset for CI pre-deploy gate.
 * Run with: bunx playwright test tests/e2e/smoke.spec.ts
 * For post-deploy Vercel: BASE_URL=https://agentsfleet.net bunx playwright test tests/e2e/smoke.spec.ts
 */
test.describe("Smoke", () => {
  test("home page loads", async ({ page }) => {
    await page.goto("/");
    await expect(page).toHaveTitle(/agentsfleet/i);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  });

  test("brand favicon is wired and resolves", async ({ page }) => {
    await page.goto("/");
    const href = await page.locator('link[rel="icon"]').getAttribute("href");
    expect(href).toBe("/favicon.svg");
    const res = await page.request.get(href!);
    expect(res.status()).toBe(200);
  });

  test("brand meta description names agentsfleet", async ({ page }) => {
    await page.goto("/");
    const description = await page
      .locator('meta[name="description"]')
      .getAttribute("content");
    expect(description).toContain("agentsfleet");
  });

  test("pricing section renders inline on home", async ({ page }) => {
    await page.goto("/#pricing");
    await expect(page.getByTestId("pricing-block")).toBeVisible();
    await expect(page.getByTestId("pricing-card-trial")).toBeVisible();
    await expect(page.getByTestId("pricing-card-usage")).toBeVisible();
    await expect(page.getByTestId("pricing-card-enterprise")).toBeVisible();
    await expect(page.getByTestId("pricing-rate-event")).toHaveText("free");
    await expect(page.getByTestId("pricing-rate-run")).toHaveText("$0.0001/sec");
    await expect(page.getByTestId("pricing-rate-run-hourly")).toHaveText("$0.36/hr");
  });

  test("llms surfaces are generated and reachable", async ({ page }) => {
    const llms = await page.request.get("/llms.txt");
    expect(llms.status()).toBe(200);
    const llmsBody = await llms.text();
    expect(llmsBody).toContain("# agentsfleet");
    expect(llmsBody).toContain("## Product");
    expect(llmsBody).toContain("[OpenAPI](/openapi.json)");

    const llmsFull = await page.request.get("/llms-full.txt");
    expect(llmsFull.status()).toBe(200);
    expect(await llmsFull.text()).toContain("AI teammates");
  });

  test("agents page loads", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous agents.",
    );
  });

  test("privacy page loads", async ({ page }) => {
    await page.goto("/privacy");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Privacy Policy");
  });

  test("terms page loads", async ({ page }) => {
    await page.goto("/terms");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Terms of Service");
  });

  test("nav links are present on home", async ({ page }) => {
    await page.goto("/");
    const nav = page.getByRole("navigation", { name: /primary/i });
    await expect(nav.getByRole("link", { name: /^home$/i })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^pricing$/i })).toBeVisible();
    await expect(nav.getByRole("link", { name: /^docs$/i })).toBeVisible();
  });

  test("footer renders on all routes", async ({ page }) => {
    for (const route of ["/", "/agents", "/privacy", "/terms"]) {
      await page.goto(route);
      await page.waitForLoadState("domcontentloaded");
      await expect(page.getByRole("contentinfo")).toBeVisible({ timeout: 10_000 });
    }
  });

  test("Discord link uses canonical URL", async ({ page }) => {
    await page.goto("/");
    const discord = page.getByRole("contentinfo").getByRole("link", { name: /^discord$/i });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });
});
