import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

test("Models and Team use actual components at the approved scale", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?resources=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await expect(page.getByRole("heading", {name: "Models", exact: true})).toBeVisible();
    await page.getByRole("button", {name: "Create model", exact: true}).click();
    const dialog = page.getByRole("dialog");
    await dialog.getByRole("combobox", {name: "Provider", exact: true}).click();
    await page.getByRole("option", {name: "Fireworks", exact: true}).click();
    await dialog.getByRole("combobox", {name: "Model", exact: true}).click();
    await page.getByRole("option", {name: /GLM 5.2/}).click();
    await expect(dialog.getByText("Context cap (tokens)")).toBeHidden();
    await dialog.locator("summary").filter({hasText: "Model details"}).click();
    await expect(dialog.getByText("Context cap (tokens)")).toBeVisible();
    await dialog.locator("summary").filter({hasText: "Model details"}).click();
    await dialog.getByRole("button", {name: "Use saved key"}).click();
    await dialog.getByRole("combobox", {name: "Saved key"}).click();
    await page.getByRole("option", {name: "Fireworks workspace"}).click();
    await expect(dialog.getByRole("combobox", {name: "Saved key"})).toHaveText("Fireworks workspace");
    expect(await dialog.getByRole("button", {name: "Create model", exact: true}).evaluate(node => getComputedStyle(node).fontSize)).toBe("13px");
    expect(await dialog.evaluate(node => node.getBoundingClientRect().right <= innerWidth)).toBe(true);
    expect(await dialog.getByRole("button", {name: "Create model", exact: true}).evaluate(node => node.getBoundingClientRect().bottom <= innerHeight)).toBe(true);
    await page.screenshot({path: info.outputPath(`create-model-${theme}.png`), scale: "css", animations: "disabled"});
    await dialog.getByRole("button", {name: "Cancel", exact: true}).click();
    await page.getByRole("button", {name: "Team", exact: true}).click();
    await expect(page.getByRole("heading", {name: "Team", exact: true})).toBeVisible();
    await expect(page.getByText("Email not sent", {exact: true}).filter({visible: true})).toBeVisible();
    expect(await page.getByText("Kishore Neelamegam", {exact: true}).evaluate(node => getComputedStyle(node).fontSize)).toBe("13px");
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    await page.screenshot({path: info.outputPath(`team-${theme}.png`), scale: "css", animations: "disabled"});
    await page.getByRole("button", {name: "Invite", exact: true}).click();
    await expect(page.getByRole("dialog")).toBeVisible();
    await page.screenshot({path: info.outputPath(`invite-${theme}.png`), scale: "css", animations: "disabled"});
  }
  expect(errors).toEqual([]);
});
