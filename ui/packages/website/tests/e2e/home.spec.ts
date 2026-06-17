import { test, expect } from "@playwright/test";

test.describe("Home page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("renders hero heading", async ({ page }) => {
    const h1 = page.getByRole("heading", { level: 1 });
    await expect(h1).toContainText("A resident engineer that compounds operational knowledge.");
  });

  test("hero LIVE eyebrow renders a WakePulse data-live element", async ({ page }) => {
    const eyebrow = page.getByTestId("hero-eyebrow");
    await expect(eyebrow).toContainText("LIVE — wake.on.event");
    await expect(eyebrow.locator('[data-live="true"]')).toBeVisible();
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

    // Promo pill between the LIVE eyebrow and the headline links to the
    // inline /pricing anchor and surfaces the rates-pin trial-end string
    // (`RATES_DISPLAY.FREE_TRIAL_PILL` in lib/rates.ts).
    const pill = page.getByTestId("hero-promo-pill");
    await expect(pill).toBeVisible();
    await expect(pill).toHaveAttribute("href", "/pricing");
    await expect(pill).toContainText(/Free until July 31, 2026/);

    await expect(page.getByTestId("hero-cta-early-access")).toContainText("Get early access");
    await expect(page.getByTestId("hero-cta-secondary")).toContainText("See the loop");
  });

  test("renders the animated hero install Terminal", async ({ page }) => {
    const term = page.getByLabel(/install via agentsfleet\.dev/i);
    await expect(term).toBeVisible();
    await expect(term).toContainText("/agentsfleet-install-platform-ops");
  });

  test("topbar renders the install CTA + brand-mark pulse", async ({ page }) => {
    const cta = page.getByTestId("header-install-cta");
    await expect(cta).toBeVisible();

    const brandMark = page.getByTestId("brand-mark");
    await expect(brandMark).toHaveAttribute("data-live", "true");
  });

  test("renders operational knowledge and Terminal Ledger sections", async ({ page }) => {
    await expect(page.getByTestId("operational-knowledge")).toBeVisible();
    await expect(page.getByText("Recurring tickets become operational memory.")).toBeVisible();
    await expect(page.getByTestId("pipeline-diagram")).toBeVisible();
    await expect(page.getByTestId("pipeline-ledger-line-problem-class")).toContainText(
      "recurring problem class",
    );
    await expect(page.getByTestId("pipeline-human-gate")).toContainText("human approval");

    await page.setViewportSize({ width: 1280, height: 800 });
    const overflowsX = await page.evaluate(
      () => document.documentElement.scrollWidth > document.documentElement.clientWidth,
    );
    expect(overflowsX).toBe(false);
  });

  test("renders how it works steps", async ({ page }) => {
    const how = page.getByTestId("how-it-works");
    await expect(how.getByRole("heading", { name: "A signal arrives", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "The agent gathers evidence", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "The class is remembered", exact: true })).toBeVisible();
  });

  test("does not render a duplicate install block below pricing", async ({ page }) => {
    // The standalone InstallBlock was removed; the loop section now carries the
    // operational path instead of a second install pitch.
    await expect(
      page.getByRole("heading", { level: 2, name: /install agentsfleet, then run/i }),
    ).toHaveCount(0);
  });

  test("topbar Pricing link scrolls to inline pricing section", async ({ page }) => {
    await page.getByRole("navigation", { name: /primary/i }).getByRole("link", { name: /pricing/i }).click();
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
