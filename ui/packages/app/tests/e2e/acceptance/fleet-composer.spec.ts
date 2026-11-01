import type { Locator } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { withReplyPage } from "./fixtures/reply-page";
import { CHAT_COPY, editSetupLink, setupAction } from "./fixtures/private-chat";

const PART = { model: '[data-composer-part="model"]', hint: '[data-composer-part="hint"]' } as const;
const SIZES = { input: "14px", model: "12px", hint: "12px" } as const;
const CASES = [
  { name: "desktop dark", width: 1440, theme: "dark", reducedMotion: "no-preference" },
  { name: "desktop light", width: 1440, theme: "light", reducedMotion: "reduce" },
  { name: "mobile dark", width: 390, theme: "dark", reducedMotion: "no-preference" },
] as const;

for (const scenario of CASES) {
  test(`shared composers retain typography and instruction-dialog focus on ${scenario.name}`, async ({ page }) => {
    await page.setViewportSize({ width: scenario.width, height: 900 });
    await page.emulateMedia({ reducedMotion: scenario.reducedMotion });
    await withReplyPage(page, "support-triage", async () => {
      await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), scenario.theme);
      const runtime = page.getByLabel("Chat composer", { exact: true });
      const runtimeInput = runtime.getByRole("textbox");
      const inputStyle = await typography(runtimeInput);
      const modelStyle = await typography(runtime.locator(PART.model));
      const hintStyle = await typography(runtime.locator("..").locator(PART.hint));
      await editSetupLink(page).click();
      const setup = page.getByLabel(CHAT_COPY.composer, {exact: true});
      const setupInput = setup.getByRole("textbox");
      await expect(setupInput).toBeEnabled();
      expect(await typography(setupInput)).toEqual(inputStyle);
      expect(await typography(setup.locator(PART.model))).toEqual(modelStyle);
      expect(await typography(setup.locator("..").locator(PART.hint))).toEqual(hintStyle);
      expect(inputStyle.fontSize).toBe(SIZES.input);
      expect(modelStyle.fontSize).toBe(SIZES.model);
      expect(hintStyle.fontSize).toBe(SIZES.hint);
      expect(inputStyle.fontFamily).toContain("Instrument Sans");
      await expect(page.getByRole("button", { name: "Save changes", exact: true })).toHaveCount(1);
      await setupInput.fill("Keep my unsent setup edit");
      await (await setupAction(page, CHAT_COPY.editSetup)).click();
      const editor = page.getByRole("dialog", {name: CHAT_COPY.instructions, exact: true});
      await expect(editor.getByLabel(CHAT_COPY.instructions, {exact: true})).toBeVisible();
      await page.keyboard.press("Escape");
      await expect(editor).toBeHidden();
      const opener = page.getByRole("button", {name: "Setup chat actions", exact: true});
      await expect(opener).toBeFocused();
      await expect(setupInput).toHaveValue("Keep my unsent setup edit");
      await (await setupAction(page, CHAT_COPY.editSetup)).click();
      await editor.getByRole("button", {name: "Cancel", exact: true}).click();
      await expect(editor).toBeHidden();
      await expect(opener).toBeFocused();
      await expect(setupInput).toHaveValue("Keep my unsent setup edit");
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
      expect(overflow).toBe(false);
    });
  });
}

function typography(locator: Locator) {
  return locator.evaluate(element => {
    const style = getComputedStyle(element);
    return { fontFamily: style.fontFamily, fontSize: style.fontSize, lineHeight: style.lineHeight, fontWeight: style.fontWeight };
  });
}
