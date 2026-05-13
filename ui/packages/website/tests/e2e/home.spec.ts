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
    const install = page.getByTestId("hero-cta-primary");
    await expect(install).toBeVisible();
    await expect(install).toHaveAttribute("href", /docs\.usezombie\.com\/quickstart/);

    const replay = page.getByTestId("hero-cta-secondary");
    await expect(replay).toBeVisible();
    await expect(replay).toHaveAttribute("href", "/agents");

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

  test("renders feature flow rows", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Install once." })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Every event on the record." })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
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
