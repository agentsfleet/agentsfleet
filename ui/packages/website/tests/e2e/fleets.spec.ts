import { expect, test } from "@playwright/test";

test("the agents page shows how to start a fleet in the terminal or with the API", async ({ page }) => {
  await page.goto("/agents");
  const terminal = page.getByLabel("Fleet commands");
  await expect(terminal).toContainText('agentsfleet fleet create "Check my latest deploy');
  await expect(terminal).toContainText("agentsfleet fleet steer @deploy-watch");
  await expect(page.getByRole("link", { name: "OpenAPI JSON" })).toHaveAttribute("href", "/openapi.json");
  await expect(page.getByRole("heading", { name: "Create a fleet" })).toBeVisible();
  await expect(page.getByRole("contentinfo")).toBeVisible();
});
