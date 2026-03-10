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

async function clickInternalAndReturn(page: Page, link: InternalLinkCase, originPath: string) {
  const scope = page.getByRole("contentinfo");
  await scope.getByRole("link", { name: link.label, exact: true }).click();
  await expect(page).toHaveURL(new RegExp(`${link.href.replace("/", "\\/")}$`));
  if (link.heading) {
    await expect(page.getByRole("heading", { level: 1 })).toContainText(link.heading, { timeout: 10_000 });
  }
  if (link.href === originPath) {
    return;
  }
  await page.goBack();
  await expect(page).toHaveURL(new RegExp(`${originPath === "/" ? "\\/$" : originPath.replace("/", "\\/") + "$"}`));
}

async function assertFooterLinks(page: Page, originPath: string) {
  const footer = page.getByRole("contentinfo");
  await expect(footer).toBeVisible();

  const internalFooterLinks: InternalLinkCase[] = [
    { label: "Features", href: "/" },
    { label: "Pricing", href: "/pricing", heading: "Free and Scale plans" },
    { label: "Agents", href: "/agents", heading: "This page is for autonomous agents." },
    { label: "Privacy", href: "/privacy", heading: "Privacy Policy" },
    { label: "Terms", href: "/terms", heading: "Terms of Service" },
  ];

  for (const link of internalFooterLinks) {
    await clickInternalAndReturn(page, link, originPath);
  }

  await expect(footer.getByRole("link", { name: "Docs" })).toHaveAttribute("href", "https://docs.usezombie.com");
  await expect(footer.getByRole("link", { name: "GitHub" })).toHaveAttribute("href", "https://github.com/usezombie/usezombie");
  await expect(footer.getByRole("link", { name: "Discord" })).toHaveAttribute("href", "https://discord.gg/H9hH2nqQjh");
}

test.describe("Humans vs Agents exhaustive link traversal", () => {
  test("Humans page: nav links, in-page links, and footer links are traversable", async ({ page }) => {
    await page.goto("/");
    await expectMode(page, "humans");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("Ship AI-generated PRs");

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing$/);
    await page.goBack();
    await expect(page).toHaveURL(/\/$/);

    await nav.getByRole("link", { name: "Agents" }).click();
    await expect(page).toHaveURL(/\/agents$/);
    await page.goBack();
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

    await assertFooterLinks(page, "/");
  });

  test("Agents page: nav links, machine links, install links, and footer links are traversable", async ({ page }) => {
    await page.goto("/agents");
    await expectMode(page, "agents");
    await expect(page.getByRole("heading", { level: 1 })).toContainText("This page is for autonomous agents.");

    const nav = page.getByRole("navigation", { name: /primary/i });
    await nav.getByRole("link", { name: "Home" }).click();
    await expect(page).toHaveURL(/\/$/);
    await page.goBack();
    await expect(page).toHaveURL(/\/agents$/);

    await nav.getByRole("link", { name: "Pricing" }).click();
    await expect(page).toHaveURL(/\/pricing$/);
    await page.goBack();
    await expect(page).toHaveURL(/\/agents$/);

    await expect(nav.getByRole("link", { name: "Docs" })).toHaveAttribute("href", "https://docs.usezombie.com");

    await expect(page.getByRole("link", { name: "Install Zombiectl" })).toHaveAttribute("href", "https://docs.usezombie.com/quickstart");
    await expect(page.getByRole("link", { name: "Read the docs" })).toHaveAttribute("href", "https://docs.usezombie.com");
    await expect(page.getByRole("link", { name: "Setup your personal dashboard" })).toHaveAttribute(
      "href",
      /app\.(dev\.)?usezombie\.com/
    );

    const contractPaths = ["/openapi.json", "/agent-manifest.json", "/skill.md", "/llms.txt", "/heartbeat"];
    for (const endpoint of contractPaths) {
      await page.getByRole("link", { name: endpoint, exact: true }).click();
      await expect(page).toHaveURL(new RegExp(`${endpoint.replace("/", "\\/")}$`));
      await page.goBack();
      await expect(page).toHaveURL(/\/agents$/);
    }

    await assertFooterLinks(page, "/agents");
  });
});
