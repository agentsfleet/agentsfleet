import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";

test("Security combines keys and sessions with compact type in both appearances", async ({page, chatProbe}, info) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  await page.goto(`${chatProbe}?accountSettings=1&avatar=1`);
  await expect(page.getByRole("region", {name: "Your account"})).toHaveCount(0);
  await expect(page.getByText("Signed in as", {exact: true})).toHaveCount(0);
  await expect(page.getByRole("button", {name: "Profile and sign-in settings"})).toHaveCount(0);
  await expect(page.getByText("platform-priming", {exact: true})).toBeVisible();
  await expect(page.getByText("Chrome on macOS")).toBeVisible();
  await expect(page.getByText("This device", {exact: true})).toBeVisible();
  await expect(page.getByText("Last active unavailable")).toBeVisible();
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    await expect(page.getByRole("heading", {name: "Security", exact: true})).toBeVisible();
    await expect(page.getByRole("heading", {name: "API keys", exact: true})).toBeVisible();
    await expect(page.getByRole("heading", {name: "Active sessions", exact: true})).toBeVisible();
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const button = page.getByRole("button", {name: "Create API key", exact: true});
    expect(await button.evaluate(element => getComputedStyle(element).fontFamily)).toContain("Manrope");
    expect(await button.evaluate(element => getComputedStyle(element).fontSize)).toBe("13px");
    const account = page.getByRole("button", {name: /^Workspace and account menu/});
    const image = account.locator("[data-account-avatar] img");
    await expect(image).toBeVisible();
    expect(await image.evaluate(element => (element as HTMLImageElement).naturalWidth)).toBeGreaterThan(0);
    await account.hover();
    await expect(page.getByRole("tooltip")).toHaveText("you@example.test");
    const path = info.outputPath(`security-${theme}.png`);
    await page.screenshot({path, fullPage: true, scale: "css", animations: "disabled"});
    await info.attach(`Security-${theme}`, {path, contentType: "image/png"});
  }
  expect(errors).toEqual([]);
});

test("remote session sign-out requires confirmation and unavailable sessions can be retried", async ({page, chatProbe}) => {
  await page.goto(`${chatProbe}?accountSettings=1`);
  const signOut = page.getByRole("button", {name: "Sign out Safari on iPhone"});
  await signOut.click();
  await expect(page.getByRole("alertdialog")).toBeVisible();
  await page.getByRole("button", {name: "Cancel", exact: true}).click();
  await expect(page.getByText("Safari on iPhone")).toBeVisible();
  await page.getByRole("button", {name: "Errors", exact: true}).click();
  await expect(page.getByText("Sessions could not be loaded. Try again.")).toBeVisible();
  await page.getByRole("button", {name: "Errors", exact: true}).click();
  await expect(signOut).toBeVisible();
  await signOut.click();
  await page.getByRole("alertdialog").getByRole("button", {name: "Sign out session", exact: true}).click();
  await expect(page.getByText("Safari on iPhone")).toBeHidden();
  await expect(page.getByText("This device", {exact: true})).toBeVisible();
});
