import { expect, type Locator } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { DEPLOY_CHECK } from "./fixtures/deploy-failure-data";
import { visualWakeup } from "./fixtures/refresh-wakeup-data";

const views = [{state: "error", reference: "failure", name: "Deploy monitor"}, {state: "wake-up", reference: "scheduled", name: "Merge watch"}] as const;
const type = ["font-family", "font-size", "line-height", "font-weight", "color"];
const frame = ["background-color", "border-top-color", "border-top-width", "border-top-left-radius", "padding-top", "padding-right", "padding-bottom", "padding-left", "width"];

for (const view of views) test(`${view.state} follows the reference on desktop and phone`, async ({page, chatProbe, isMobile}, info) => {
  const size = isMobile ? {width: 390, height: 844} : {width: 1440, height: 1000};
  const reference = await page.context().newPage();
  for (const surface of [page, reference]) {await surface.setViewportSize(size); await surface.emulateMedia({reducedMotion: "reduce"});}
  await Promise.all([page.goto(`${chatProbe}?visual=1&state=${view.state}`), reference.goto(new URL(`../../../../../docs/ux.html#${view.reference}`, import.meta.url).href)]);
  const alert = page.getByRole("alert", {name: "Couldn’t complete this request"});
  const strip = page.getByRole("group", {name: "Every 30 minutes"}).locator("..");
  const events = page.locator('[data-role="system"]');
  const design = reference.locator(view.state === "error" ? ".error-card" : ".wake-strip");
  const actual = view.state === "error" ? alert : strip;
  await expect(actual).toBeVisible(); await expect(design).toBeVisible();
  const chat = page.getByRole("log", {name: "Chat"});
  for (const theme of ["dark", "light"]) {
    for (const surface of [page, reference]) await surface.evaluate(async theme => {document.documentElement.dataset.theme = theme; await document.fonts.ready;}, theme);
    await match(actual, design, frame);
    await match(page.getByRole("heading", {name: view.name, exact: true}), reference.locator(".title-group h1"), type);
    await match(chat.locator('[data-role="user"] p').first(), reference.locator(".user-bubble p"), type);
    await match(chat.locator('[data-role="assistant"] p').first(), reference.locator(".assistant-turn > p").first(), type);
    await match(page.getByRole("textbox", {name: /Message/i}).first(), reference.locator(".composer textarea"), ["font-family", "font-size", "line-height"]);
    if (view.state === "error") {
      const heading = alert.getByRole("heading", {name: "Couldn’t complete this request"});
      const primary = alert.getByRole("button", {name: "Try again"});
      await match(heading, design.locator("h2"), [...type, "gap", "margin-bottom"]);
      await match(heading.locator("svg"), design.locator("h2 i"), ["width", "height"]);
      await match(alert.locator("p").first(), design.locator("p").first(), type);
      await match(primary, design.locator(".primary"), [...type, "height", "background-color", "border-radius", "padding-top", "padding-left"]);
      await match(alert.getByRole("button", {name: "View details"}), design.locator("summary"), type);
      expect(await verticalGap(alert.locator("p").first(), primary)).toBe(await verticalGap(design.locator("p").first(), design.locator(".primary")));
    } else {
      const receipts = reference.locator(".event-line");
      await match(strip, design, [...type, "margin-left", "margin-right", "margin-bottom"]);
      await match(strip.locator('fieldset > div > span').first(), design.locator(":scope > span"), type);
      await match(strip.getByRole("button", {name: "Edit wake-up"}).locator("svg"), design.locator("button i").first(), ["width", "height"]);
      for (let i=0; i<2; i++) {
        await match(events.nth(i), receipts.nth(i), [...type, "gap", "width", "padding-top", "padding-bottom"]);
        await match(events.nth(i).locator("p span").first(), receipts.nth(i).locator("strong"), type);
      }
      expect(await verticalGap(events.first(), events.nth(1))).toBe(await verticalGap(receipts.first(), receipts.nth(1)));
      await expect(events.first().locator("time")).toHaveCSS("display", isMobile ? "none" : "block");
      const action = await strip.getByRole("button", {name: "Edit wake-up"}).boundingBox();
      expect(action?.width).toBe(28); expect(action?.height).toBe(isMobile ? 44 : 28);
    }
    const appAnchor = isMobile ? view.state === "error" ? alert : events.first() : chat.locator("[data-conversation-date]").first();
    const refAnchor = isMobile ? view.state === "error" ? design : reference.locator(".event-line").first() : reference.locator(".day-marker").first();
    for (const [label, surface, anchor] of [["app", page, appAnchor], ["reference", reference, refAnchor]] as const) {
      await anchor.evaluate((el, mobile) => el.scrollIntoView({block: mobile ? "center" : "start", behavior: "instant"}), isMobile);
      await expect(anchor).toBeInViewport();
      expect(await surface.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      const path = info.outputPath(`${label}-${view.state}-${theme}.png`);
      await surface.screenshot({path, animations: "disabled", scale: "css"});
      await info.attach(`${label}-${view.state}-${theme}`, {path, contentType: "image/png"});
    }
  }
  if (view.state === "error") {
    const details = alert.getByRole("button", {name: "View details"});
    await details.focus(); await details.press("Enter");
    await expect(alert.getByText("Reason: transport_loss")).toBeVisible();
    await details.press("Space"); await expect(details).toHaveAttribute("aria-expanded", "false");
    await alert.getByRole("button", {name: "Try again"}).press("Enter");
    await expect(chat.locator('[data-role="user"]').filter({hasText: DEPLOY_CHECK})).toHaveCount(2);
    await expect(alert).toBeVisible();
  } else {
    const schedule = visualWakeup(Date.now());
    await page.route(`**/schedules/${schedule.schedule_id}`, route => route.fulfill({json: {...schedule, source: "api", etag: '"sample"'}}));
    await strip.getByRole("button", {name: "Edit wake-up"}).press("Enter");
    await expect(page.getByRole("dialog", {name: "Edit wake-up"}).getByLabel("Timezone")).toHaveValue("UTC");
    await page.keyboard.press("Escape");
    await strip.getByRole("button", {name: "Delete wake-up"}).press("Space");
    await expect(page.getByRole("alertdialog", {name: "Delete wake-up?"})).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(strip).toBeVisible();
  }
  await reference.close();
});

async function match(actual: Locator, reference: Locator, properties: string[]) {
  const styles = (element: Locator) => element.evaluate((node, names) => Object.fromEntries(names.map(name => {
    const value = getComputedStyle(node).getPropertyValue(name);
    // The app and the self-contained prototype register the same variable face under different names.
    return [name, name === "font-family" ? value.split(",")[0]?.replaceAll('"', "").replace(" Variable", "") : value];
  })), properties);
  await expect.poll(() => styles(actual), {message: actual.toString()}).toEqual(await styles(reference));
}

async function verticalGap(first: Locator, second: Locator) {
  const a = (await first.boundingBox())!, b = (await second.boundingBox())!;
  return Math.round(b.y - a.y - a.height);
}
