import { test, expect, type Page } from "@playwright/test";

type InternalLinkCase = {
  label: RegExp;
  href: string;
  heading?: string;
};

async function assertFooterLinks(page: Page) {
  const footer = page.getByRole("contentinfo");
  await expect(footer).toBeVisible();

  const dashboardHref = await page.getByTestId("header-install-cta").getAttribute("href");
  expect(dashboardHref).toMatch(/^https?:\/\//);
  await expect(footer.getByRole("link", { name: "dashboard", exact: true })).toHaveAttribute("href", dashboardHref!);
  await expect(footer.getByRole("link", { name: /^early access$/i })).toHaveCount(0);

  const internalFooterLinks: InternalLinkCase[] = [
    { label: /^Use cases$/i, href: "/#operational-loop" },
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
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "AI teammates for incident response.",
    );

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: /^how it works$/i }).click();
    await expect(page).toHaveURL(/\/#how-it-works$/);
    await page.goto("/");
    await expect(page).toHaveURL(/\/$/);

    await nav.getByRole("link", { name: /^agents$/i }).click();
    await expect(page).toHaveURL(/\/agents$/);
    await page.goto("/");
    await expect(page).toHaveURL(/\/$/);

    await expect(nav.getByRole("link", { name: /^docs$/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );

    await expect(page.getByRole("button", { name: /copy the install command/i })).toHaveCount(0);
    await expect(page.getByTestId("hero-cta-early-access")).toHaveAttribute("href", /\/waitlist$/);

    await page.getByTestId("hero-promo-pill").click();
    await expect(page).toHaveURL(/\/#pricing$/);
    await expect(page.getByTestId("pricing-block")).toBeVisible();

    await expect(
      page.getByTestId("hero").getByRole("link", { name: /talk to us/i }),
    ).toHaveCount(0);
    await expect(page.getByTestId("pricing-cta-early-access")).toHaveText(/request early access/i);

    await assertFooterLinks(page);
  });

  test("Fleets page exposes expected machine and install links", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for agents.",
    );

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: /^home$/i }).click();
    await expect(page).toHaveURL(/\/$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await nav.getByRole("link", { name: /^how it works$/i }).click();
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
