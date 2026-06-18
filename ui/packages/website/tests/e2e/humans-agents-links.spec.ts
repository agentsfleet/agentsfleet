import { test, expect, type Page } from "@playwright/test";

type InternalLinkCase = {
  label: RegExp;
  href: string;
  heading?: string;
};

async function assertFooterLinks(page: Page) {
  const footer = page.getByRole("contentinfo");
  await expect(footer).toBeVisible();

  const internalFooterLinks: InternalLinkCase[] = [
    { label: /^fleet$/i, href: "/#operational-loop" },
    { label: /^pricing$/i, href: "/#pricing" },
    { label: /^agents$/i, href: "/agents" },
    { label: /^llms\.txt$/i, href: "/llms.txt" },
    { label: /^llms-full\.txt$/i, href: "/llms-full.txt" },
    { label: /^OpenAPI$/, href: "/openapi.json" },
    { label: /^privacy$/i, href: "/privacy" },
    { label: /^terms$/i, href: "/terms" },
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
      "A resident engineer that compounds operational knowledge.",
    );

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: /^pricing$/i }).click();
    await expect(page).toHaveURL(/\/#pricing$/);
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

    // The Hero primary copy affordance is a clipboard-copy button, not a docs anchor.
    const heroCtaPrimary = page.getByTestId("hero-cta-primary");
    await expect(heroCtaPrimary).toHaveJSProperty("tagName", "BUTTON");
    await expect(heroCtaPrimary).not.toHaveAttribute("href", /./);

    // The Hero promo pill is the home page's link to the inline /pricing anchor.
    await expect(page.getByTestId("hero-promo-pill")).toHaveAttribute("href", "/pricing");

    await expect(
      page.getByTestId("hero").getByRole("link", { name: /talk to us/i }),
    ).toHaveCount(0);
    await expect(page.getByTestId("pricing-cta-enterprise")).toHaveText(/talk to us/i);

    await assertFooterLinks(page);
  });

  test("Agents page exposes expected machine and install links", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for autonomous agents.",
    );

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: /^home$/i }).click();
    await expect(page).toHaveURL(/\/$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await nav.getByRole("link", { name: /^pricing$/i }).click();
    await expect(page).toHaveURL(/\/#pricing$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await expect(nav.getByRole("link", { name: /^docs$/i })).toHaveAttribute(
      "href",
      "https://docs.agentsfleet.net",
    );

    await expect(
      page.locator('a[href="https://docs.agentsfleet.net/quickstart"]').filter({ hasText: /start an agent/i }),
    ).toHaveCount(1);
    await expect(
      page.locator('a[href="https://docs.agentsfleet.net"]').filter({ hasText: /read the docs/i }),
    ).toHaveCount(1);
    await expect(
      page.locator("a").filter({ hasText: /open dashboard/i }),
    ).toHaveAttribute("href", /app\.(dev\.)?agentsfleet\.net/);

    await expect(page.getByTestId("agents-openapi-link")).toHaveAttribute("href", "/openapi.json");

    await assertFooterLinks(page);
  });
});
