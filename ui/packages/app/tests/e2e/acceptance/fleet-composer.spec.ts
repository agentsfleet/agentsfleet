import type { Locator } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { EDIT_SETUP_PLACEHOLDER } from "@/components/domain/chat/chat-defaults";
import { withReplyPage } from "./fixtures/reply-page";

const PART = { model: '[data-composer-part="model"]', hint: '[data-composer-part="hint"]' } as const;
const SIZES = { input: "15px", model: "12px", hint: "12px" } as const;
const CASES = [
  { name: "desktop dark", width: 1440, theme: "dark", reducedMotion: "no-preference" },
  { name: "desktop light", width: 1440, theme: "light", reducedMotion: "reduce" },
  { name: "mobile dark", width: 390, theme: "dark", reducedMotion: "no-preference" },
] as const;

for (const scenario of CASES) {
  test(`shared composers retain typography and editor focus on ${scenario.name}`, async ({ page }) => {
    await page.setViewportSize({ width: scenario.width, height: 900 });
    await page.emulateMedia({ reducedMotion: scenario.reducedMotion });
    await withReplyPage(page, "support-triage", async () => {
      await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), scenario.theme);
      const runtime = page.getByLabel("Chat composer", { exact: true });
      const runtimeInput = runtime.getByRole("textbox");
      await runtimeInput.fill("Keep my unsent question");
      const inputStyle = await typography(runtimeInput);
      const modelStyle = await typography(runtime.locator(PART.model));
      const hintStyle = await typography(runtime.locator("..").locator(PART.hint));
      const opener = page.getByRole("button", { name: "Edit fleet setup", exact: true });
      await opener.click();
      const editor = page.getByRole("complementary", { name: "Edit fleet setup" });
      const setup = editor.getByLabel("Private chat composer", { exact: true });
      const setupInput = setup.getByLabel(EDIT_SETUP_PLACEHOLDER);
      await expect(setupInput).toBeEnabled();
      await expect(editor.getByRole("heading", { name: "Edit your fleet" })).toBeVisible();
      expect(await typography(setupInput)).toEqual(inputStyle);
      expect(await typography(setup.locator(PART.model))).toEqual(modelStyle);
      expect(await typography(setup.locator("..").locator(PART.hint))).toEqual(hintStyle);
      expect(inputStyle.fontSize).toBe(SIZES.input);
      expect(modelStyle.fontSize).toBe(SIZES.model);
      expect(hintStyle.fontSize).toBe(SIZES.hint);
      expect(inputStyle.fontFamily).toContain("Instrument Sans");
      await expect(page.getByRole("button", { name: "Save changes", exact: true })).toHaveCount(1);
      await expect(editor.getByRole("button", { name: "Save changes", exact: true })).toHaveCount(0);
      await setupInput.fill("Keep my unsent setup edit");
      await page.keyboard.press("Escape");
      await expect(editor).toBeHidden();
      await expect(opener).toBeFocused();
      await expect(runtimeInput).toHaveValue("Keep my unsent question");
      await opener.click();
      await expect(setupInput).toHaveValue("Keep my unsent setup edit");
      await editor.getByRole("button", { name: "Close setup editor" }).click();
      await expect(opener).toBeFocused();
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
