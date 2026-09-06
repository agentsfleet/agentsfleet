import { test, expect } from "@playwright/test";

test.describe("Agents page (/agents)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/agents");
  });

  test("renders Fleet-first heading", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 1 })).toContainText(
      "This page is for agents.",
    );
  });

  test("renders the merged install heading and npm command", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /install agentsfleet/i })).toBeVisible();
    await expect(page.getByLabel(/bootstrap commands/i)).toContainText(
      "npm install -g @agentsfleet/cli",
    );
  });

  test("renders install action links and no dashboard link", async ({ page }) => {
    await expect(page.getByRole("link", { name: /start a Fleet/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /read the docs/i })).toBeVisible();
    await expect(page.getByRole("link", { name: /open dashboard/i })).toHaveCount(0);
  });

  test("renders bootstrap commands", async ({ page }) => {
    const block = page.getByLabel(/bootstrap commands/i);
    await expect(block).toBeVisible();
    await expect(block).toContainText("npm install -g @agentsfleet/cli");
    await expect(block).toContainText("agentsfleet login");
    await expect(block).toContainText("npx skills add agentsfleet/skills");
    await expect(block).toContainText("Create a fleet for incident response in my workspace.");
    await expect(block).not.toContainText("/agentsfleet-install-platform-ops");
    await expect(block).toContainText("curl -fsSL https://agentsfleet.dev | bash");
  });

  test("renders machine surface heading + openapi link", async ({ page }) => {
    await expect(page.getByRole("heading", { name: /machine surface/i })).toBeVisible();
    await expect(page.getByTestId("fleets-openapi-link")).toHaveAttribute("href", "/openapi.json");
  });
  test("renders safety limits as a constraint table", async ({ page }) => {
    await expect(page.getByRole("rowheader", { name: /^idempotency$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^audit trail$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^secret management$/i })).toBeVisible();
    await expect(page.getByRole("rowheader", { name: /^policy enforcement$/i })).toBeVisible();
  });

  test("does not render orange-era decorative chrome", async ({ page }) => {
    await expect(page.locator(".scanline")).toHaveCount(0);
    await expect(page.locator(".fleet-surface")).toHaveCount(0);
    await expect(page.locator(".fleet-table")).toHaveCount(0);
  });

  test("footer renders on fleets page", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
  });
});

test("installation precedes API steps and stale sections are absent", async ({ page }) => {
  await page.goto("/agents");
  const installHeading = "Install agentsfleet";
  const apiHeading = "Get started in four calls";
  await expect(page.getByRole("heading", { name: installHeading, exact: true })).toBeVisible();
  await expect(page.getByRole("heading", { name: apiHeading, exact: true })).toBeVisible();
  const headings = (await page.getByRole("heading").allTextContents()).map(text => text.trim());
  expect(headings.indexOf(installHeading)).toBeGreaterThanOrEqual(0);
  expect(headings.indexOf(apiHeading)).toBeGreaterThanOrEqual(0);
  expect(headings.indexOf(installHeading)).toBeLessThan(headings.indexOf(apiHeading));
  for (const name of ["Webhook ingest example", "Coming soon", "API operations"]) {
    await expect(page.getByRole("heading", { name, exact: true })).toHaveCount(0);
  }
});
