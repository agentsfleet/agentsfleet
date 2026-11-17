import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

test("setup policy help opens by touch and keyboard in both themes", async ({page, chatProbe}, info) => {
  const writes: string[] = [], errors: string[] = [];
  page.on("request", request => {if (!["GET", "HEAD"].includes(request.method())) writes.push(request.method());});
  page.on("pageerror", error => errors.push(error.message));
  for (const theme of ["dark", "light"]) {
    await page.goto(`${chatProbe}?setupAlignment=1`);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    const help = page.getByRole("button", {name: "About package manager domains", exact: true});
    await help.scrollIntoViewIfNeeded();
    if (info.project.name === "mobile-chromium") await help.tap();
    else {await help.focus(); await page.keyboard.press("Enter");}
    const dialog = page.getByRole("dialog", {name: "Package manager domains", exact: true});
    await expect(dialog).toBeVisible();
    await expect(dialog).toContainText("Allow standard package registries while public internet access is restricted.");
    await expect(dialog).toContainText("Its selection is kept when Allow all is switched off.");
    const done = dialog.getByRole("button", {name: "Done", exact: true});
    await expect(done).toBeInViewport();
    await page.screenshot({path: info.outputPath(`package-manager-help-${theme}.png`), scale: "css", animations: "disabled"});
    if (info.project.name === "mobile-chromium") await done.tap();
    else await page.keyboard.press("Escape");
    await expect(dialog).toBeHidden();
    await expect(help).toBeFocused();
  }
  expect(writes).toEqual([]);
  expect(errors).toEqual([]);
});
