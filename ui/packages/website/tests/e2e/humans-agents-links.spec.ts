import { test, expect, type Page } from "@playwright/test";

type InternalLinkCase = {
  label: string;
  href: string;
  heading?: string;
};

async function expectMode(page: Page, mode: "humans" | "agents") {
  const humans = page.getByTestId("mode-humans");
  const agents = page.getByTestId("mode-agents");
  await expect(humans).toHaveAttribute("aria-selected", mode === "humans" ? "true" : "false");
  await expect(agents).toHaveAttribute("aria-selected", mode === "agents" ? "true" : "false");
}

async function assertFooterLinks(page: Page) {
  const footer = page.getByRole("contentinfo");
  await expect(footer).toBeVisible();

  const internalFooterLinks: InternalLinkCase[] = [
    { label: "Features", href: "/" },
    { label: "Pricing", href: "/pricing", heading: "Start free. Upgrade when you need stronger control." },
    { label: "Agents", href: "/agents", heading: "This page is for autonomous agents." },
    { label: "Privacy", href: "/privacy", heading: "Privacy Policy" },
    { label: "Terms", href: "/terms", heading: "Terms of Service" },
  ];

  for (const link of internalFooterLinks) {
    await expect(footer.getByRole("link", { name: link.label, exact: true })).toHaveAttribute("href", link.href);
  }

  await expect(footer.locator('a[href^="https://docs.usezombie.com"]')).toHaveCount(1);
  await expect(footer.locator('a[href="https://github.com/usezombie/usezombie"]')).toHaveCount(1);
  await expect(footer.locator('a[href="https://discord.gg/H9hH2nqQjh"]')).toHaveCount(1);
}

test.describe("Humans vs Agents link coverage", () => {
  test("Humans page exposes expected internal and external links", async ({ page }) => {
    await page.goto("/");
    await expectMode(page, "humans");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Operational knowledge lives in heads");

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing$/);
    await page.goto("/");
    await expect(page).toHaveURL(/\/$/);

    await nav.getByRole("link", { name: "Agents" }).click();
    await expect(page).toHaveURL(/\/agents$/);
    await page.goto("/");
    await expect(page).toHaveURL(/\/$/);

    await expect(nav.getByRole("link", { name: "Docs" })).toHaveAttribute("href", "https://docs.usezombie.com");

    await expect(page.getByRole("link", { name: /connect github, automate prs/i }).first()).toHaveAttribute(
      "href",
      /app\.(dev\.)?usezombie\.com/
    );
    await expect(page.getByRole("link", { name: /talk to us/i })).toHaveCount(0);
    await expect(page.getByRole("link", { name: /install guide/i })).toHaveAttribute(
      "href",
      "https://docs.usezombie.com/quickstart"
    );
    await assertFooterLinks(page);
  });

  test("Agents page exposes expected machine and install links", async ({ page }) => {
    await page.goto("/agents");
    await expectMode(page, "agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("This page is for autonomous agents.");

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: "Home" }).click();
    await expect(page).toHaveURL(/\/$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await nav.getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing$/);
    await page.goto("/agents");
    await expect(page).toHaveURL(/\/agents$/);

    await expect(nav.getByRole("link", { name: "Docs" })).toHaveAttribute("href", "https://docs.usezombie.com");

    await expect(page.locator('a[href="https://docs.usezombie.com/quickstart"]').filter({ hasText: "Install Zombiectl" })).toHaveCount(1);
    await expect(page.locator('a[href="https://docs.usezombie.com"]').filter({ hasText: "Read the docs" })).toHaveCount(1);
    await expect(page.locator("a").filter({ hasText: "Setup your personal dashboard" })).toHaveAttribute(
      "href",
      /app\.(dev\.)?usezombie\.com/
    );

    const contractPaths = ["/openapi.json"];
    for (const endpoint of contractPaths) {
      await expect(page.locator(`a[href="${endpoint}"]`)).toHaveCount(1);
    }
    await assertFooterLinks(page);
  });
});
