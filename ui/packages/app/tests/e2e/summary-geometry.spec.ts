import { expect, type Locator } from "@playwright/test";
import { test } from "./fixtures/chat-probe";

const reference = new URL("../../../../../docs/ux.html#scheduled", import.meta.url).href;
const longSecret = "RELEASE_CHECKLIST_READ_ONLY_DEPLOYMENT_TOKEN";

test("Wake-up Summary aligns the headings, rows, switches and secret mask", async ({page, chatProbe, isMobile}, info) => {
  const prototype = await page.context().newPage();
  await Promise.all([
    page.goto(`${chatProbe}?visual=1&state=wake-up`),
    prototype.goto(reference),
  ]);
  const surfaces = [page, prototype];
  const app = page.getByRole("complementary", {name: "Summary"});
  const design = prototype.getByRole("complementary", {name: "Fleet summary"});
  const widths = isMobile ? [390, 360, 320] : [1440, 1100, 800, 681];
  await expect(app.getByText("github", {exact: true})).toBeVisible();

  for (const theme of ["dark", "light"]) {
    for (const surface of surfaces) await surface.evaluate(mode => { document.documentElement.dataset.theme = mode; }, theme);
    for (const width of widths) {
      for (const surface of surfaces) {
        await surface.setViewportSize({width, height: 1000});
        await surface.evaluate(() => document.fonts.ready);
      }
      const [actual, sample] = await Promise.all([summaryGeometry(app), summaryGeometry(design)]);
      expect(actual).toEqual(sample);
      for (const {terms, icons, aligned, packageFits, horizontalOverflow, helpHeight} of [actual, sample]) {
        expect(terms).toEqual(["Spending limit", "Next wake-up"]);
        expect(icons.every(([left, width, text, center]) => left === 0 && width === 16 && text === 24 && center === 0)).toBe(true);
        expect(aligned).toBe(true);
        expect(packageFits).toBe(true);
        expect(horizontalOverflow).toBe(false);
        expect(helpHeight).toBe(isMobile ? 44 : 32);
      }
      const [spending, wakeup] = actual.centers;
      if (spending === undefined || wakeup === undefined) throw new Error("Expected spending limit and wake-up rows");
      expect(wakeup - spending).toBe(28);
      if (width === 1440 || width === 800 || width === 320) {
        await app.screenshot({path: info.outputPath(`app-summary-${width}-${theme}.png`), animations: "disabled", scale: "css"});
        await design.screenshot({path: info.outputPath(`reference-summary-${width}-${theme}.png`), animations: "disabled", scale: "css"});
      }
    }
  }
  await prototype.close();
});

test("Overflowing secret names scroll and keep the mask and icon stationary", async ({page, chatProbe, isMobile}, info) => {
  const width = isMobile ? 320 : 1440;
  await page.setViewportSize({width, height: 1000});
  await page.emulateMedia({reducedMotion: "no-preference"});
  await page.goto(`${chatProbe}?visual=1&state=wake-up&secretLabel=${longSecret}`);
  const row = page.getByRole("listitem").filter({hasText: longSecret});
  const label = row.locator(".summary-scroll-label");
  const text = label.getByText(longSecret, {exact: true});
  const pinned = () => row.evaluate(el => Array.from(el.querySelectorAll("svg, code"), item => item.getBoundingClientRect().x));
  const offset = () => text.evaluate(el => {
    const viewport = el.parentElement;
    if (!viewport) throw new Error("Expected a scroll container for the Summary label");
    return Math.round(el.getBoundingClientRect().left - viewport.getBoundingClientRect().left);
  });

  await expect(label).toHaveAttribute("tabindex", "0");
  await expect(text).toHaveCSS("white-space", "nowrap");
  await expect(row.getByText("••••••••", {exact: true})).toBeVisible();
  const overflow = await label.evaluate(el => el.scrollWidth - el.clientWidth);
  expect(overflow).toBeGreaterThan(20);
  const before = await pinned();
  if (!isMobile) {
    await label.hover();
    await expect.poll(offset).toBeCloseTo(-overflow, 0);
    await label.evaluate(el => { el.scrollLeft = el.scrollWidth; });
    expect(await label.evaluate(el => el.scrollLeft)).toBe(0);
    expect(await pinned()).toEqual(before);
    await page.mouse.move(0, 0);
  }

  await page.emulateMedia({reducedMotion: "reduce"});
  await expect(text).toHaveCSS("transform", "none");
  if (!isMobile) {
    await page.getByRole("link", {name: "Manage secrets", exact: true}).focus();
    await page.keyboard.press("Tab");
    await expect(label).toBeFocused();
    await page.keyboard.press("ArrowRight");
    await expect.poll(() => label.evaluate(el => el.scrollLeft)).toBeGreaterThan(0);
  }
  await label.evaluate(el => { el.scrollLeft = el.scrollWidth; });
  expect(await label.evaluate(el => el.scrollLeft)).toBe(overflow);
  expect(await pinned()).toEqual(before);
  await row.screenshot({path: info.outputPath(`secret-name-ending-${width}.png`), scale: "css"});
});

async function summaryGeometry(panel: Locator) {
  return panel.evaluate(element => {
    const title = element.querySelector("h2");
    const label = element.querySelector('label[for$="-packages"], label[for="setup-package-domains"]');
    if (!title || !label?.nextElementSibling) throw new Error("Expected the Summary title and package controls");
    const heading = title.getBoundingClientRect();
    const help = label.nextElementSibling.getBoundingClientRect();
    const range = document.createRange();
    range.selectNodeContents(label);
    const center = (el: Element) => {
      const {y, height} = el.getBoundingClientRect();
      return Math.round(y + height / 2 - heading.y);
    };
    const terms = [...element.querySelectorAll("dt")];
    const rows = [...terms, ...element.querySelectorAll("li")];
    const frame = getComputedStyle(element);
    return {
      frame: [Math.round(element.getBoundingClientRect().width), frame.paddingLeft, frame.paddingRight, frame.borderTopLeftRadius],
      terms: terms.map(term => term.textContent?.trim()),
      centers: terms.map(center),
      sections: [...element.querySelectorAll("h3")].map(center),
      icons: rows.map(row => {
        const icon = row.querySelector("svg");
        const text = row.children[1];
        if (!icon || !text) throw new Error("Expected a Summary icon and label");
        const bounds = icon.getBoundingClientRect();
        return [Math.round(bounds.left - heading.left), Math.round(bounds.width),
          Math.round(text.getBoundingClientRect().left - heading.left), center(icon) - center(text)];
      }),
      aligned: [...element.querySelectorAll('dd, code, [role="switch"], a[aria-label^="Allow all"] > [data-state]')].every(el =>
        Math.round(el.getBoundingClientRect().right - heading.right) === 0),
      packageFits: range.getClientRects().length === 1 && range.getBoundingClientRect().right <= help.left,
      helpHeight: help.height,
      horizontalOverflow: element.scrollWidth > element.clientWidth,
    };
  });
}
