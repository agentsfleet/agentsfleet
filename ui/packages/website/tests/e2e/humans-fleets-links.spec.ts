import { test, expect, type Page } from "@playwright/test";

const WAITLIST = "https://accounts.agentsfleet.net/waitlist";

type InternalLinkCase = {
  label: RegExp;
  href: string;
};

async function assertFooterLinks(page: Page) {
  const footer = page.getByRole("contentinfo");
  await expect(footer).toBeVisible();

  const dashboardHref = await page.getByTestId("header-install-cta").getAttribute("href");
  expect(dashboardHref).toMatch(/^https?:\/\//);
  await expect(footer.getByRole("link", { name: "Join waitlist", exact: true })).toHaveAttribute("href", dashboardHref!);
  await expect(footer.getByRole("link", { name: /^early access$/i })).toHaveCount(0);

  const internalFooterLinks: InternalLinkCase[] = [
    { label: /^Examples$/i, href: "/examples" },
    { label: /^Agents$/i, href: "/agents" },
    { label: /^privacy$/i, href: "/privacy" },
    { label: /^terms$/i, href: "/terms" },
    { label: /^contact$/i, href: "mailto:agentsfleet@agentmail.to" },
  ];

  for (const link of internalFooterLinks) {
    await expect(footer.getByRole("link", { name: link.label })).toHaveAttribute("href", link.href);
  }

  await expect(footer.locator('a[href^="https://docs.agentsfleet.net"]')).toHaveCount(1);
  await expect(footer.locator('a[href="https://github.com/agentsfleet/agentsfleet"]')).toHaveCount(1);
  await expect(footer.locator('a[href="https://discord.gg/H9hH2nqQjh"]')).toHaveCount(1);
}

test.describe("Cross-page link coverage", () => {
  test("Home page exposes expected internal and external links", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Find out why. Check the fix.");
    const nav = page.getByRole("navigation", { name: "Primary" });
    await nav.getByRole("link", { name: "Product" }).click();
    await expect(page).toHaveURL(/\/#how-it-works$/);
    await expect(page.getByRole("heading", { name: /work in a private network/ })).toBeInViewport();
    await expect(nav.getByRole("link", { name: "Docs" })).toHaveAttribute("href", "https://docs.agentsfleet.net");
    const heroLinks = page.getByRole("region", { name: /Find out why\. Check the fix\./ }).getByRole("link");
    await expect(heroLinks).toHaveText(["Join waitlist", "View on GitHub"]);
    await expect(heroLinks.first()).toHaveAttribute("href", WAITLIST);
    await expect(heroLinks.last()).toHaveAttribute("href", "https://github.com/agentsfleet/agentsfleet");
    await expect(page.getByTestId("header-install-cta")).toHaveAttribute("href", WAITLIST);
    await expect(page.getByTestId("pricing-cta-early-access")).toHaveAttribute("href", WAITLIST);
    const footer = page.getByRole("contentinfo");
    await footer.getByRole("link", { name: "Examples" }).click();
    await expect(page.getByRole("textbox", { name: "What should your fleet handle?" })).toBeVisible();
  });

  test("Fleets page exposes expected machine and install links", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for agents.",
    );

    const nav = page.getByRole("navigation", { name: /primary/i });
    await page.getByTestId("brand-link").click();
    await expect(page).toHaveURL(/\/$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await nav.getByRole("link", { name: /^Product$/i }).click();
    await expect(page).toHaveURL(/\/#how-it-works$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await expect(nav.getByRole("link", { name: /^docs$/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );

    await expect(
      page.locator('a[href="https://docs.agentsfleet.net/quickstart"]').filter({ hasText: /start a Fleet/i }),
    ).toHaveCount(1);
    await expect(
      page.locator('a[href="https://docs.agentsfleet.net"]').filter({ hasText: /read the docs/i }),
    ).toHaveCount(1);
    // "open dashboard" was removed from the merged install block.
    await expect(
      page.locator("a").filter({ hasText: /open dashboard/i }),
    ).toHaveCount(0);

    await expect(page.getByTestId("fleets-openapi-link")).toHaveAttribute("href", "/openapi.json");

    await assertFooterLinks(page);
  });
});
