import { test, expect } from "@playwright/test";

test.describe("Pricing page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/pricing");
  });

  test("renders pricing heading", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "Start free. Upgrade when you need stronger control.",
    );
  });

  test("renders current pricing tiers", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Hobby", exact: true, level: 2 })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Scale", exact: true, level: 2 })).toBeVisible();
  });

  test("highlights the upgrade tier with a pulse-bordered card", async ({ page }) => {
    await expect(page.getByText(/^upgrade when ready$/)).toBeVisible();
    const scaleCard = page.getByTestId("pricing-card-scale");
    await expect(scaleCard).toHaveAttribute("data-featured", "true");
    // Card's `featured` variant emits `border-primary` (= var(--pulse));
    // the page also adds `hover:border-primary` so the accent survives
    // the base `hover:border-border-strong` override.
    await expect(scaleCard).toHaveClass(/border-primary/);
    await expect(scaleCard).toHaveClass(/hover:border-primary/);
  });

  test("Hobby tier lists no-expiry credit", async ({ page }) => {
    await expect(page.getByText(/\$5 starter credit, never expires/i)).toBeVisible();
  });

  test("renders move-up guidance note", async ({ page }) => {
    await expect(page.getByText(/Start on Hobby/i)).toBeVisible();
  });

  test("FAQ accordion: first item opens on click", async ({ page }) => {
    const trigger = page.getByTestId("faq-trigger-0");
    await expect(trigger).toBeVisible();
    await expect(page.getByTestId("faq-answer-0")).not.toBeVisible();
    await trigger.click();
    await expect(page.getByTestId("faq-answer-0")).toBeVisible();
  });

  test("FAQ accordion: closes on second click", async ({ page }) => {
    const trigger = page.getByTestId("faq-trigger-0");
    await trigger.click();
    await expect(page.getByTestId("faq-answer-0")).toBeVisible();
    await trigger.click();
    await expect(page.getByTestId("faq-answer-0")).not.toBeVisible();
  });

  test("FAQ accordion: only one item open at a time", async ({ page }) => {
    await page.getByTestId("faq-trigger-0").click();
    await expect(page.getByTestId("faq-answer-0")).toBeVisible();
    await page.getByTestId("faq-trigger-1").click();
    await expect(page.getByTestId("faq-answer-1")).toBeVisible();
    await expect(page.getByTestId("faq-answer-0")).not.toBeVisible();
  });

  test("Hobby start CTA links to mission control", async ({ page }) => {
    const cta = page.getByRole("link", { name: /start free/i }).first();
    await expect(cta).toHaveAttribute("href", "https://app.dev.usezombie.com");
  });

  test("Scale tier exposes an app upgrade action", async ({ page }) => {
    const cta = page.getByRole("link", { name: /upgrade in app/i });
    await expect(cta).toBeVisible();
    await expect(
      page.getByText(/operator-visible upgrade path after free credit exhaustion/i),
    ).toBeVisible();
  });

  test("footer renders on pricing page", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
  });
});
