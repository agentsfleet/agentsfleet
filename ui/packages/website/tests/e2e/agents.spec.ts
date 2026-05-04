import { test, expect } from "@playwright/test";

test.describe("Agents page (/agents)", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/agents");
  });

  test("renders agent-first heading", async ({ page }) => {
    await expect(page.getByRole("heading", { level: 1 })).toContainText("autonomous agents");
  });

  test("renders install block with npm command", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Install Zombiectl" })).toBeVisible();
    await expect(page.getByLabel(/install zombiectl command/i)).toContainText(
      "npm install -g @usezombie/zombiectl"
    );
  });

  test("renders install block action buttons", async ({ page }) => {
    await expect(page.getByRole("link", { name: "Install platform-ops" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Read the docs" })).toBeVisible();
    await expect(page.getByRole("link", { name: "Setup your personal dashboard" })).toBeVisible();
  });

  test("renders bootstrap commands", async ({ page }) => {
    const block = page.getByLabel(/bootstrap commands/i);
    await expect(block).toBeVisible();
    await expect(block).toContainText("npm install -g @usezombie/zombiectl");
    await expect(block).toContainText("zombiectl auth login");
    await expect(block).toContainText("/usezombie-install-platform-ops");
  });

  test("renders machine surface table with all endpoints", async ({ page }) => {
    await expect(page.getByText("Machine Surface")).toBeVisible();
    await expect(page.getByRole("link", { name: "/openapi.json" })).toBeVisible();
  });

  test("renders API operations table", async ({ page }) => {
    await expect(page.getByText("API Operations")).toBeVisible();
    await expect(page.getByText("Create agent")).toBeVisible();
    await expect(page.getByText("Kill agent")).toBeVisible();
    await expect(page.getByText("Steer / chat")).toBeVisible();
    await expect(page.getByText("Pause workspace")).toBeVisible();
    await expect(page.getByText("Execute tool")).toHaveCount(0);
  });

  test("renders webhook example", async ({ page }) => {
    await expect(page.getByText("Webhook Ingest Example")).toBeVisible();
    await expect(page.getByText(/workflow_run\.failed/)).toBeVisible();
  });

  test("renders safety limit cards", async ({ page }) => {
    await expect(page.getByRole("heading", { name: "Idempotency" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Audit Trail" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Secret Management" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Policy Enforcement" })).toBeVisible();
  });

  test("uses monospace/terminal aesthetic", async ({ page }) => {
    const surface = page.locator(".agent-surface");
    await expect(surface).toBeVisible();
  });

  test("footer renders on agents page", async ({ page }) => {
    await expect(page.getByRole("contentinfo")).toBeVisible();
  });
});
