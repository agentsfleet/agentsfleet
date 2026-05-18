import { test, expect } from "@playwright/test";

test.describe("Home page", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
  });

  test("renders hero heading (Mockup A two-line)", async ({ page }) => {
    const h1 = page.getByRole("heading", { level: 1 });
    await expect(h1).toContainText("Your deploy failed.");
    await expect(h1).toContainText("The agent already knows why.");
  });

  test("hero LIVE eyebrow renders a WakePulse data-live element", async ({ page }) => {
    const eyebrow = page.getByTestId("hero-eyebrow");
    await expect(eyebrow).toContainText("LIVE — wake.on.event");
    await expect(eyebrow.locator('[data-live="true"]')).toBeVisible();
  });

  test("renders hero CTAs", async ({ page }) => {
    // Primary CTA is a clipboard-copy button that carries the install
    // one-liner as its visible label — not a docs anchor.
    const install = page.getByTestId("hero-cta-primary");
    await expect(install).toBeVisible();
    await expect(install).toHaveJSProperty("tagName", "BUTTON");
    await expect(install).not.toHaveAttribute("href", /./);
    await expect(install).toContainText(
      "npm install -g @usezombie/zombiectl && npx skills add usezombie/usezombie",
    );

    const replay = page.getByTestId("hero-cta-secondary");
    await expect(replay).toBeVisible();
    await expect(replay).toHaveAttribute("href", "/agents");

    // Promo pill between the LIVE eyebrow and the headline links to the
    // inline /pricing anchor and surfaces the rates-pin trial-end string
    // (`RATES_DISPLAY.FREE_TRIAL_PILL` in lib/rates.ts).
    const pill = page.getByTestId("hero-promo-pill");
    await expect(pill).toBeVisible();
    await expect(pill).toHaveAttribute("href", "/pricing");
    await expect(pill).toContainText(/Free until July 31, 2026/);

    await expect(page.getByRole("link", { name: /talk to us/i })).toHaveCount(0);
  });

  test("renders hero install transcript Terminal", async ({ page }) => {
    const term = page.getByLabel(/install platform-ops via claude code/i);
    await expect(term).toBeVisible();
    await expect(term).toContainText("/usezombie-install-platform-ops");
  });

  test("topbar renders the install CTA + brand-mark pulse", async ({ page }) => {
    const cta = page.getByTestId("header-install-cta");
    await expect(cta).toBeVisible();

    const brandMark = page.getByTestId("brand-mark");
    await expect(brandMark).toHaveAttribute("data-live", "true");
  });

  test("renders OnboardingFlow steps", async ({ page }) => {
    // `OnboardingFlow.tsx` is the home page's four-step pictorial. Anchors at
    // `#onboarding-flow` for the Hero CTA's smooth-scroll target.
    const flow = page.locator('[aria-label="Onboarding flow"]');
    await expect(flow).toBeVisible();
    await expect(flow.getByRole("heading", { name: "Install the CLI" })).toBeVisible();
    await expect(flow.getByRole("heading", { name: "Run the install skill" })).toBeVisible();
    await expect(flow.getByRole("heading", { name: "Wire your trigger" })).toBeVisible();
    await expect(flow.getByRole("heading", { name: "Steer your zombie" })).toBeVisible();
  });

  test("renders how it works steps", async ({ page }) => {
    const how = page.getByTestId("how-it-works");
    await expect(how.getByRole("heading", { name: "A trigger arrives", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "The agent gathers evidence", exact: true })).toBeVisible();
    await expect(how.getByRole("heading", { name: "Diagnosis posts; the run is auditable", exact: true })).toBeVisible();
  });

  test("renders final install block actions", async ({ page }) => {
    await expect(
      page.getByRole("heading", { level: 2, name: "Install zombiectl, then run /usezombie-install-platform-ops" }),
    ).toBeVisible();
    await expect(page.getByRole("link", { name: /read the docs/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /start an agent/i })).toHaveAttribute(
      "href",
      /docs\.usezombie\.com\/quickstart/,
    );
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
    const discord = footer.getByRole("link", { name: /^discord$/i });
    await expect(discord).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
  });
});
