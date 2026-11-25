import type { Locator } from "@playwright/test";
import { expect, test } from "@playwright/test";
import { withReplyPage } from "./fixtures/reply-page";
import { NEW_FLEET_COPY } from "./fixtures/constants";
import { workspaceHref } from "./fixtures/nav";

const PART = { model: '[data-composer-part="model"]', hint: '[data-composer-part="hint"]' } as const;
const SIZES = {model: "12px", hint: "12px"} as const;
const CASES = [
  { name: "desktop dark", width: 1440, inputSize: "13px", inputLine: "20px", theme: "dark", reducedMotion: "no-preference" },
  { name: "desktop light", width: 1440, inputSize: "13px", inputLine: "20px", theme: "light", reducedMotion: "reduce" },
  { name: "mobile dark", width: 390, inputSize: "16px", inputLine: "24px", theme: "dark", reducedMotion: "no-preference" },
] as const;

for (const scenario of CASES) {
  test(`the fleet chat and New fleet share one composer's typography on ${scenario.name}`, async ({ page }) => {
    await page.setViewportSize({ width: scenario.width, height: 900 });
    await page.emulateMedia({ reducedMotion: scenario.reducedMotion });
    await withReplyPage(page, "support-triage", async ({workspaceId}) => {
      await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), scenario.theme);
      const runtime = page.getByLabel("Chat composer", { exact: true });
      const runtimeInput = runtime.getByRole("textbox");
      const inputStyle = await typography(runtimeInput);
      const modelStyle = await typography(runtime.locator(PART.model));
      const hintStyle = await typography(runtime.locator("..").locator(PART.hint));
      expect(inputStyle.fontSize).toBe(scenario.inputSize);
      expect(inputStyle.lineHeight).toBe(scenario.inputLine);
      expect(modelStyle.fontSize).toBe(SIZES.model);
      expect(hintStyle.fontSize).toBe(SIZES.hint);
      expect(inputStyle.fontFamily).toContain("Manrope");

      // New fleet types its first message into the same composer.
      await page.goto(workspaceHref(workspaceId, "fleets/new"));
      await page.evaluate(theme => document.documentElement.setAttribute("data-theme", theme), scenario.theme);
      const creating = page.getByLabel(NEW_FLEET_COPY.composer, {exact: true});
      const creatingInput = creating.getByPlaceholder(NEW_FLEET_COPY.placeholder);
      await expect(creatingInput).toBeEnabled();
      expect(await typography(creatingInput)).toEqual(inputStyle);
      expect(await typography(creating.locator(PART.model))).toEqual(modelStyle);
      expect(await typography(creating.locator("..").locator(PART.hint))).toEqual(hintStyle);
      await expect(page.getByRole("button", { name: "Save fleet", exact: true })).toHaveCount(0);
      await creatingInput.fill("Keep my unsent first message");
      await expect(creatingInput).toHaveValue("Keep my unsent first message");
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
