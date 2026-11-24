import { expect, type Locator } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { DEPLOY_CHECK } from "./fixtures/deploy-failure-data";
import { visualWakeup } from "./fixtures/refresh-wakeup-data";

const views = [{state: "error", reference: "failure", name: "Deploy monitor"}, {state: "wake-up", reference: "scheduled", name: "Merge watch"}] as const;
const type = ["font-family", "font-size", "line-height", "font-weight", "color"];
const frame = ["background-color", "border-top-color", "border-top-width", "border-top-left-radius", "padding-top", "padding-right", "padding-bottom", "padding-left", "width"];
const referenceTime = new Date("2026-11-15T12:00:00Z");

test.use({timezoneId: "UTC"});

for (const view of views) test(`${view.state} follows the reference on desktop and phone`, async ({page, chatProbe, isMobile}, info) => {
  const size = isMobile ? {width: 390, height: 844} : {width: 1440, height: 1000};
  const reference = await page.context().newPage();
  // The reference shows one day of activity.
  await page.clock.setFixedTime(referenceTime);
  for (const surface of [page, reference]) {
    await surface.setViewportSize(size);
    await surface.emulateMedia({reducedMotion: "reduce"});
  }
  await Promise.all([page.goto(`${chatProbe}?visual=1&state=${view.state}`), reference.goto(new URL(`../../../../../docs/ux.html#${view.reference}`, import.meta.url).href)]);
  const alert = page.getByRole("alert", {name: "Couldn’t complete this request"});
  const strip = page.getByRole("group", {name: "Every 30 minutes"}).locator("..");
  const events = page.locator('[data-role="system"]');
  const design = reference.locator(view.state === "error" ? ".retry-note" : ".wake-strip");
  const actual = view.state === "error" ? alert : strip;
  await expect(actual).toBeVisible(); await expect(design).toBeVisible();
  const chat = page.getByRole("log", {name: "Chat"});
  const input = page.getByRole("textbox", {name: /Message/i}).first();
  if (view.state === "wake-up") await expect(input).toBeFocused();
  for (const theme of ["dark", "light"]) {
    for (const surface of [page, reference]) await surface.evaluate(async theme => {document.documentElement.dataset.theme = theme; await document.fonts.ready;}, theme);
    await match(actual, design, view.state === "error" ? ["background-color", "border-top-width"] : frame);
    await match(page.getByRole("heading", {name: view.name, exact: true}), reference.locator(".title-group h1"), type);
    await match(chat.locator('[data-role="user"] p').first(), reference.locator(".user-bubble p"), type);
    await match(chat.locator('[data-role="assistant"] p').first(), reference.locator(".assistant-turn > p").first(), type);
    await match(input, reference.locator(".composer textarea"), ["font-family", "font-size", "line-height"]);
    if (view.state === "error") {
      await expect(alert.getByText(/deployment connection expired before the health check/i)).toBeVisible();
      const primary = alert.getByRole("button", {name: "Try again"});
      await match(primary.locator("svg"), design.locator(".retry-action i"), ["width", "height"]);
      await expect(primary).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
      await expect(alert.getByRole("button", {name: "View details"})).toBeVisible();
    } else {
      const receipts = reference.locator(".event-line");
      const summary = page.getByRole("complementary", {name: "Summary"});
      await expect(summary.getByText("MiniMax M3.1 Flash")).toBeVisible();
      await expect(summary.getByText("$1.00 / day")).toBeVisible();
      await expect(summary.locator("dd time")).toBeVisible();
      await expect(summary.getByText("agentsfleet/agentsfleet", {exact: true})).toBeVisible();
      const publicAccess = summary.getByRole("link", {name: /Allow all public internet access \(on\)/});
      const packages = summary.getByRole("switch", {name: "Enable package manager domains"});
      await expect(publicAccess).toBeVisible();
      await expect(packages).toBeDisabled();
      await expect(packages).toBeChecked();
      await expect(summary.getByText("github", {exact: true})).toBeVisible();
      const designSummary = reference.locator(".summary");
      await match(summary.getByRole("heading", {name: "Summary"}), designSummary.locator("h2"), type);
      for (const label of ["Model", "Spending limit", "Next wake-up", "Repositories", "Public internet access", "Secrets"]) {
        await match(summary.getByText(label, {exact: true}), designSummary.getByText(label, {exact: true}), ["font-size", "color"]);
      }
      const switchStyle = ["width", "height", "background-color", "opacity"];
      await match(publicAccess.locator("[data-state]").first(), designSummary.locator("#setup-public-internet"), switchStyle);
      await match(packages, designSummary.getByRole("switch", {name: "Enable package manager domains"}), switchStyle);
      if (!isMobile) await match(summary, designSummary, ["width", "background-color", "border-top-color", "border-top-left-radius"]);
      await match(strip, design, [...type, "margin-left", "margin-right", "margin-bottom"]);
      const seam = await composerSeam(strip, page.locator(".fleet-composer-surface"));
      expect(seam).toEqual(await composerSeam(design, reference.locator(".composer")));
      expect(seam).toEqual(isMobile ? [8, 8, -1] : [20, 20, -1]);
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
    const schedule = visualWakeup(referenceTime.getTime());
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

async function composerSeam(strip: Locator, composer: Locator) {
  const a = (await strip.boundingBox())!;
  const b = (await composer.boundingBox())!;
  return [Math.round(a.x - b.x), Math.round(b.x + b.width - a.x - a.width), Math.round(b.y - a.y - a.height)];
}
