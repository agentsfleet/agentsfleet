import { test, expect } from "@playwright/test";

test.describe("Home page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("renders hero heading", async ({ page }) => {
    const h1 = page.getByRole("heading", { level: 1 });
    await expect(h1).toContainText("Keep shipping. Bring a fleet.");
  });

  test("hero illustration does not imply live activity", async ({ page }) => {
    const eyebrow = page.getByTestId("hero-eyebrow");
    await expect(eyebrow).toContainText("Engineering work");
    await expect(eyebrow.locator('[data-live="true"]')).toHaveCount(0);
    const artwork = page.getByRole("img", { name: /Three agents collaborate/ });
    await expect(artwork).toBeVisible();
    await expect(artwork).toHaveJSProperty("complete", true);
    expect(await artwork.evaluate((img) => (img as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
  });

  test("renders hero CTAs", async ({ page }) => {
    // The install one-liner sits in a copy-row; the primary CTA is a
    // copy-only button (no docs anchor, no scroll).
    const command = page.getByTestId("hero-install-command");
    await expect(command).toContainText("curl -fsSL https://agentsfleet.dev | bash");
    const install = page.getByTestId("hero-cta-primary");
    await expect(install).toBeVisible();
    await expect(install).toHaveJSProperty("tagName", "BUTTON");
    await expect(install).not.toHaveAttribute("href", /./);
    await expect(install).toContainText(/copy/i);

    // The promo pill opens early-access information without quoting a price.
    const pill = page.getByTestId("hero-promo-pill");
    await expect(pill).toBeVisible();
    await expect(pill).toHaveAttribute("href", "/#pricing");
    await expect(pill).toContainText(/help shape agentsfleet/i);
    await expect(pill).not.toContainText(/\$\d|starter credit/i);

    const earlyAccess = page.getByTestId("hero-cta-early-access");
    await expect(earlyAccess).toContainText("Request early access");
    // Now an enabled link to the Clerk-hosted waitlist (was a disabled button).
    await expect(earlyAccess).toHaveAttribute("href", /\/waitlist$/);
    await expect(page.getByTestId("hero-cta-secondary")).toContainText("Meet the fleet");
  });

  test("no longer renders the removed hero install Terminal", async ({ page }) => {
    await expect(page.getByLabel(/install via agentsfleet\.dev/i)).toHaveCount(0);
    await expect(page.getByTestId("hero-cli")).toHaveCount(0);
  });

  test("topbar renders the waitlist CTA + brand-mark pulse", async ({ page }) => {
    const cta = page.getByTestId("header-install-cta");
    await expect(cta).toBeVisible();
    await expect(cta).toHaveAttribute("href", /\/waitlist$/);

    const brandMark = page.getByTestId("brand-mark");
    await expect(brandMark).toHaveAttribute("data-live", "true");
  });

  test("keeps the Fleet catalogue without repetitive knowledge or setup sections", async ({ page }) => {
    await expect(page.getByTestId("operational-knowledge")).toHaveCount(0);
    await expect(page.getByTestId("setup-section")).toHaveCount(0);
    await expect(page.getByTestId("prebuilt-fleets")).toBeVisible();
    await expect(page.getByTestId("fleet-card-auto-reviewer")).toContainText("PR Reviewer");
    await expect(page.getByTestId("fleet-card-security-reviewer")).toContainText("Security Reviewer");
    await expect(page.getByTestId("fleet-coming-soon-security-reviewer")).toContainText(/coming soon/i);
    await expect(page.getByTestId("fleet-card-slack-teammate")).toContainText(/mention-only and read-only/i);

    await page.setViewportSize({ width: 1280, height: 800 });
    const overflowsX = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    );
    expect(overflowsX).toBe(false);
  });

  test("Fleet cards stay visible without horizontal overflow on mobile", async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await expect(page.getByTestId("prebuilt-fleets")).toBeVisible();
    for (const id of ["auto-reviewer", "diagnose", "security-reviewer"]) {
      await expect(page.getByTestId(`fleet-card-${id}`)).toBeVisible();
    }
    const overflowsX = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    );
    expect(overflowsX).toBe(false);
  });

  test("renders how it works steps", async ({ page }) => {
    const how = page.getByTestId("how-it-works");
    await expect(how.getByText("01 / Gather evidence", { exact: true })).toBeVisible();
    await expect(how.getByText("02 / Investigate", { exact: true })).toBeVisible();
    await expect(how.getByText("03 / You approve", { exact: true })).toBeVisible();
    await expect(how.getByText("05 / GitHub draft PR", { exact: true })).toBeVisible();
    await expect(how).toContainText("A diagnosis alone never starts it.");
  });

  test("does not render a duplicate install block below pricing", async ({ page }) => {
    // The standalone InstallBlock was removed; the loop section now carries the
    // operational path instead of a second install pitch.
    await expect(
      page.getByRole("heading", { level: 2, name: /install agentsfleet, then run/i }),
    ).toHaveCount(0);
  });

  test("topbar Pricing link scrolls to inline pricing section", async ({ page }) => {
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: /^early access$/i }).click();
    await expect(page).toHaveURL(/\/#pricing$/);
    await expect(page.getByTestId("pricing-block")).toBeVisible();
  });

  test("footer is present with canonical Discord URL", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
    const footer = page.getByRole("contentinfo");
    await expect(footer.getByRole("link", { name: /^github$/i })).toBeVisible();
    await expect(footer.getByRole("link", { name: /^llms\.txt$/i })).toHaveAttribute(
      "href",
      "/llms.txt",
    );
    await expect(footer.getByRole("link", { name: /^llms-full\.txt$/i })).toHaveAttribute(
      "href",
      "/llms-full.txt",
    );
    const discord = footer.getByRole("link", { name: /^discord$/i });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });
});
