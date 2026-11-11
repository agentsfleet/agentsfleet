import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";

test("personal settings preserve compact type and structured spacing in both appearances", async ({page, chatProbe}, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?accountSettings=1`);
  await expect(page.getByText("Chrome on macOS")).toBeVisible();
  await expect(page.getByText("This device")).toBeVisible();
  await expect(page.getByText("Last active unavailable")).toBeVisible();
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    for (const view of ["Security", "Profile"]) {
      await page.getByRole("button", {name: view, exact: true}).click();
      await expect(page.getByRole("heading", {name: view, exact: true})).toBeVisible();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      const button = page.getByRole("button", {name: view === "Profile" ? "Save changes" : "Refresh sessions"});
      expect(await button.evaluate(element => getComputedStyle(element).fontFamily)).toContain("Manrope");
      expect(await button.evaluate(element => getComputedStyle(element).fontSize)).toBe("13px");
      const path = info.outputPath(`${view.toLowerCase()}-${theme}.png`);
      await page.screenshot({path, fullPage: true, scale: "css", animations: "disabled"});
      await info.attach(`${view}-${theme}`, {path, contentType: "image/png"});
    }
  }
  expect(errors).toEqual([]);
});

test("profile errors retain edits and remote session sign-out requires confirmation", async ({page, chatProbe}) => {
  await page.goto(`${chatProbe}?accountSettings=1`);
  await page.getByRole("button", {name: "Sign out Safari on iPhone"}).click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.getByRole("button", {name: "Cancel", exact: true}).click();
  await expect(page.getByText("Safari on iPhone")).toBeVisible();
  await page.getByRole("button", {name: "Profile", exact: true}).click();
  await page.getByRole("textbox", {name: "First name"}).fill("Indy");
  await page.getByRole("button", {name: "Errors", exact: true}).click();
  await page.getByRole("button", {name: "Save changes"}).click();
  await expect(page.getByText("Profile save unavailable. Your edited names are retained.")).toBeVisible();
  await expect(page.getByRole("textbox", {name: "First name"})).toHaveValue("Indy");
  await page.getByRole("button", {name: "Errors", exact: true}).click();
  await page.getByRole("button", {name: "Save changes"}).click();
  await expect(page.getByText("Profile saved", {exact: true})).toBeVisible();
});
