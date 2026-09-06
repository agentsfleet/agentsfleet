import { test, expect, type Page } from "@playwright/test";

const AUTH_ROUTES = ["/sign-in", "/sign-up"];
const INPUT_SELECTOR = 'input[name="identifier"], input[name="emailAddress"]';
const RENDER_TIMEOUT = 30_000;

async function tokenColor(page: Page, token: string) {
  return page.evaluate((name) => {
    const probe = document.createElement("span");
    probe.style.color = `var(${name})`;
    document.body.append(probe);
    const color = getComputedStyle(probe).color;
    probe.remove();
    return color;
  }, token);
}

async function visibleGradients(page: Page) {
  return page.evaluate(() => [...document.querySelectorAll("body *")]
    .filter(element => element.checkVisibility())
    .flatMap(element => [null, "::before", "::after"].flatMap(pseudo => {
      const style = getComputedStyle(element, pseudo);
      return style.backgroundImage.includes("gradient") && (!pseudo || style.content !== "none")
        ? [`${element.className}${pseudo ?? ""}`] : [];
    })));
}

for (const route of AUTH_ROUTES) {
  test(`${route} renders shared fonts, solid surfaces, and visible input states`, async ({ page }) => {
    await page.goto(route);
    const input = page.locator(INPUT_SELECTOR).first();
    await expect(input).toBeVisible({ timeout: RENDER_TIMEOUT });
    await page.evaluate(() => document.fonts.ready);
    const pulse = await tokenColor(page, "--pulse");
    const strongBorder = await tokenColor(page, "--border-strong");
    await expect(input).toHaveCSS("border-width", "1px");
    await expect(input).toHaveCSS("border-color", strongBorder);
    await expect(input).toHaveCSS("font-family", /Instrument Sans Variable/);
    await input.hover();
    await expect(input).toHaveCSS("border-color", pulse);
    await input.focus();
    await page.keyboard.press("Tab");
    await page.keyboard.press("Shift+Tab");
    await expect(input).toBeFocused();
    await expect(input).toHaveCSS("box-shadow", `${pulse} 0px 0px 0px 2px`);
    const button = page.locator(".cl-formButtonPrimary");
    await expect(button).toHaveCSS("background-color", await tokenColor(page, "--cta"));
    await expect(button).toHaveCSS("color", await tokenColor(page, "--cta-foreground"));
    expect(await visibleGradients(page)).toEqual([]);
    const card = await page.locator(".cl-cardBox").boundingBox();
    expect(card).not.toBeNull();
    expect(card!.x).toBeGreaterThanOrEqual(0);
    expect(card!.x + card!.width).toBeLessThanOrEqual(page.viewportSize()!.width);
  });

  test(`${route} keeps invalid email correction on the local themed form`, async ({ page }) => {
    await page.goto(route);
    const input = page.locator(INPUT_SELECTOR).first();
    await expect(input).toBeVisible({ timeout: RENDER_TIMEOUT });
    await input.fill("invalid-address");
    await page.locator(".cl-formButtonPrimary").click();
    expect(await input.evaluate((element: HTMLInputElement) => element.validity.valid)).toBe(false);
    await expect(input).toBeVisible();
    expect(new URL(page.url()).pathname).toBe(route);
  });
}

test("protected dashboard opens local sign-in", async ({ page, baseURL }) => {
  await page.goto("/");
  await expect(page.locator(INPUT_SELECTOR).first()).toBeVisible({ timeout: RENDER_TIMEOUT });
  expect(new URL(page.url()).origin).toBe(new URL(baseURL!).origin);
  expect(new URL(page.url()).pathname).toBe("/sign-in");
});
