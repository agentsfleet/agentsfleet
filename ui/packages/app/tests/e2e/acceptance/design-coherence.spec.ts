import { expect, test, type Locator, type Page } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { clientFor } from "./fixtures/api-client";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { expectAccessible } from "./fixtures/accessibility";

const VIEWPORTS = [1440, 390];

async function textStart(locator: Locator, text: string) {
  return locator.evaluate((element, expected) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    while (walker.nextNode()) {
      if (walker.currentNode.textContent?.trim() !== expected) continue;
      const range = document.createRange();
      range.selectNodeContents(walker.currentNode);
      return range.getBoundingClientRect().left;
    }
    throw new Error(`Missing visible label: ${expected}`);
  }, text);
}

async function checkCreatedColumn(page: Page, name: string) {
  const table = page.getByRole("table");
  await expect(table).toBeVisible();
  await expect(table.getByRole("columnheader")).toHaveText(["Name", "Created", "Actions"]);
  const row = table.getByRole("row").filter({ hasText: name });
  await expect(row).toBeVisible();
  const created = row.getByRole("cell").nth(1).locator("time").first();
  const heading = table.getByRole("columnheader", { name: "Created" });
  await created.scrollIntoViewIfNeeded();
  const left = await textStart(heading, "Created");
  const timestamp = await created.boundingBox();
  expect(Math.abs(left - timestamp!.x)).toBeLessThan(1);
  const direction = await heading.getAttribute("aria-sort");
  const nextDirection = direction === "ascending" ? "descending" : "ascending";
  await heading.getByRole("button").click();
  await expect(heading).toHaveAttribute("aria-sort", nextDirection);
  await expect(row).toBeVisible();
  await row.getByRole("cell").last().scrollIntoViewIfNeeded();
  await expect(row.getByRole("button").last()).toBeInViewport();
  const account = await page.getByRole("button", { name: "Open user menu" }).boundingBox();
  expect(account!.x + account!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
}

test("Secrets keeps Created aligned before Actions at desktop and mobile widths", async ({ page }) => {
  const workspaceId = await getDefaultWorkspaceId("regular");
  const client = clientFor("regular");
  const name = `e2e-alignment-${crypto.randomUUID()}`;
  const endpoint = `/v1/workspaces/${workspaceId}/secrets`;
  await client.post(endpoint, { name, data: { purpose: "visual regression fixture" } });
  try {
    await signInAs(page, "regular");
    for (const width of VIEWPORTS) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto(`/w/${workspaceId}/secrets`);
      await checkCreatedColumn(page, name);
      await expectAccessible(page, "Populated secrets table");
      for (const action of ["Edit", "Rename", "Delete"]) {
        const opener = page.getByRole("button", { name: `${action} secret ${name}`, exact: true });
        await opener.click();
        const dialog = page.getByRole("dialog").or(page.getByRole("alertdialog"));
        await expect(dialog).toBeVisible();
        await expect(dialog).toHaveAccessibleName(/.+/);
        await expect(dialog).toHaveAccessibleDescription(/.+/);
        await expectAccessible(page, `${action} secret dialog`);
        await page.keyboard.press("Escape");
        await expect(opener).toBeFocused();
      }
    }
  } finally {
    await client.delete(`${endpoint}/${encodeURIComponent(name)}`);
  }
});

test("API Keys keeps Created aligned before Actions at desktop and mobile widths", async ({ page }) => {
  const client = clientFor("admin");
  const name = `e2e-alignment-${crypto.randomUUID()}`;
  const { id } = await client.post<{ id: string }>("/v1/api-keys", { key_name: name });
  try {
    await signInAs(page, "admin");
    for (const width of VIEWPORTS) {
      await page.setViewportSize({ width, height: 900 });
      await page.goto("/settings/api-keys");
      await checkCreatedColumn(page, name);
      await expectAccessible(page, "Populated API keys table");
    }
  } finally {
    await client.patch(`/v1/api-keys/${id}`, { active: false });
    await client.delete(`/v1/api-keys/${id}`);
  }
});
