import { test, expect } from "@playwright/test";

test.describe("Humans/Agents mode switch", () => {
  test("defaults to Humans mode on /", async ({ page }) => {
    await page.goto("/");
    const humansBtn = page.getByTestId("mode-humans");
    await expect(humansBtn).toHaveAttribute("aria-selected", "true");
    await expect(humansBtn).toHaveClass(/active/);
  });

  test("switches to Agents mode and navigates to /agents", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByTestId("mode-agents")).toHaveAttribute("aria-selected", "true");
    await expect(page.getByTestId("mode-humans")).toHaveAttribute("aria-selected", "false");
  });

  test("switches back to Humans mode and navigates to /", async ({ page }) => {
    await page.goto("/agents");
    await page.getByTestId("mode-humans").click();
    await expect(page).toHaveURL(/^http:\/\/[^/]+\/$/);
    await expect(page.getByTestId("mode-humans")).toHaveAttribute("aria-selected", "true");
  });

  test("mode is agents when on /agents URL", async ({ page }) => {
    await page.goto("/agents");
    await expect(page.getByTestId("mode-agents")).toHaveClass(/active/);
    await expect(page.getByTestId("mode-agents")).toHaveAttribute("aria-selected", "true");
  });

  test("mode is humans when on /pricing URL", async ({ page }) => {
    await page.goto("/pricing");
    await expect(page.getByTestId("mode-humans")).toHaveClass(/active/);
    await expect(page.getByTestId("mode-humans")).toHaveAttribute("aria-selected", "true");
  });

  test("eyebrow text changes with mode", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText(/durable agent runtime/i).first()).toBeVisible();

    await page.getByTestId("mode-agents").click();
    await expect(page).toHaveURL(/\/agents/);
    await expect(page.getByRole("heading", { level: 1 })).toContainText("autonomous agents");
  });

  test("mode switch is keyboard navigable", async ({ page }) => {
    await page.goto("/");
    await page.getByTestId("mode-humans").focus();
    await expect(page.getByTestId("mode-humans")).toBeFocused();

    await page.keyboard.press("Tab");
    await expect(page.getByTestId("mode-agents")).toBeFocused();
  });
});
