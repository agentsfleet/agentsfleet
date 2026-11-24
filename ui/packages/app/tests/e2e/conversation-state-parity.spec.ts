import { expect, type Locator, type Page } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { DEPLOY_CHECK } from "./fixtures/deploy-failure-data";
import { visualWakeup } from "./fixtures/refresh-wakeup-data";
import { expectSummaryColumns } from "./fixtures/summary-columns";

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
      await compareWakeupSummary(page, reference, isMobile);
      await match(strip, design, [...type, "margin-left", "margin-right", "margin-bottom"]);
      const seam = await composerSeam(strip, page.locator(".fleet-composer-surface"));
      expect(seam).toEqual(await composerSeam(design, reference.locator(".composer")));
      expect(seam).toEqual(isMobile ? [8, 8, 8] : [20, 20, 8]);
      await match(strip.getByText("Next wake-up ·", {exact: true}), design.locator(".wake-label-prefix"), type);
      await match(strip.getByRole("button", {name: "Edit wake-up"}).locator("svg"), design.locator("button i").first(), ["width", "height"]);
      for (let i=0; i<2; i++) {
        await match(events.nth(i), receipts.nth(i), [...type, "gap", "width", "padding-top", "padding-bottom"]);
        await match(events.nth(i).locator("p span").first(), receipts.nth(i).locator("strong"), type);
      }
      expect(await verticalGap(events.first(), events.nth(1))).toBe(await verticalGap(receipts.first(), receipts.nth(1)));
      await expect(events.first().locator("time")).toHaveCSS("display", isMobile ? "none" : "block");
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

async function compareWakeupSummary(page: Page, reference: Page, isMobile: boolean) {
  const summary = page.getByRole("complementary", {name: "Summary"});
  const prototype = reference.getByRole("complementary", {name: "Fleet summary"});
  await expect(summary.getByText("$1.00 / day")).toBeVisible();
  await expect(summary.locator("dd time")).toBeVisible();
  const publicAccess = summary.getByRole("link", {name: /Allow all public internet access \(on\)/});
  const packages = summary.getByRole("switch", {name: "Enable package manager domains"});
  await expect(publicAccess).toBeVisible();
  await expect(packages).toBeDisabled();
  await expect(packages).toBeChecked();
  await match(summary.getByRole("heading", {name: "Summary"}), prototype.locator("h2"), type);
  const sections = ["Repositories", "Public internet access", "Secrets"];
  for (const label of ["Spending limit", "Next wake-up", ...sections]) {
    await match(summary.getByText(label, {exact: true}), prototype.getByText(label, {exact: true}), ["font-size", "color"]);
  }
  for (const label of ["Allow all", "Enable package manager domains"]) {
    await match(summary.getByText(label, {exact: true}), prototype.getByText(label, {exact: true}), ["font-size", "line-height"]);
  }
  for (const panel of [summary, prototype]) {
    await expect(panel.locator("dt")).toHaveCount(2);
    await expect(panel.locator("dt").first()).toHaveText("Spending limit");
    await expectSummaryColumns(panel, sections.map(label => panel.getByText(label, {exact: true})), [
      ...await panel.locator("dt").all(),
      panel.getByText("agentsfleet/agentsfleet", {exact: true}).locator(".."),
      panel.locator("p").filter({hasText: "Tools · Shell"}),
      panel.getByText("Allow all", {exact: true}).locator(".."),
      panel.getByText("Enable package manager domains", {exact: true}).locator("../.."),
      panel.getByText("github", {exact: true}).locator(".."),
    ]);
  }
  const allowAll = publicAccess.locator("[data-state]").first();
  const designAllowAll = prototype.locator("#setup-public-internet");
  const designPackages = prototype.getByRole("switch", {name: "Enable package manager domains"});
  const switchStyle = ["width", "height", "background-color", "opacity"];
  await match(allowAll, designAllowAll, switchStyle);
  await match(packages, designPackages, switchStyle);
  await expectWakeupSummaryValues(summary, [allowAll, packages]);
  await expectWakeupSummaryValues(prototype, [designAllowAll, designPackages]);
  if (!isMobile) await match(summary, prototype, ["width", "background-color", "border-top-color", "border-top-left-radius"]);
}

async function expectWakeupSummaryValues(panel: Locator, controls: Locator[]) {
  const rightEdge = async (locator: Locator) => {
    const box = await locator.boundingBox();
    if (!box) throw new Error("The Summary value or control has no visible bounds");
    return Math.round(box.x + box.width);
  };
  const values = await panel.locator("dd").all();
  await expect(panel.locator("dd").first()).toHaveCSS("font-weight", "600");
  const right = await rightEdge(panel.getByRole("heading", {name: "Summary", exact: true}));
  for (const item of [...values, ...controls]) expect(await rightEdge(item)).toBe(right);
}

test("scheduled and paused wake-ups keep their text and controls on one line", async ({page, chatProbe, isMobile}, info) => {
  const reference = await page.context().newPage();
  await page.clock.setFixedTime(referenceTime);
  await Promise.all([
    page.goto(`${chatProbe}?visual=1&state=wake-up`),
    reference.goto(new URL("../../../../../docs/ux.html#scheduled", import.meta.url).href),
  ]);
  const appStrip = page.getByRole("group", {name: "Every 30 minutes"}).locator("..");
  const refStrip = reference.locator(".wake-strip");
  const widths = isMobile ? [390, 360, 320] : [1440];
  const surfaces = [page, reference];
  for (const theme of ["dark", "light"]) {
    for (const surface of surfaces) await surface.evaluate(async palette => {
      document.documentElement.dataset.theme = palette;
      await document.fonts.ready;
    }, theme);
    for (const width of widths) {
      await resizeWakeupDemo(surfaces, width);
      await expectWakeupLine(appStrip, appStrip.locator("time"), isMobile);
      await expectWakeupLine(refStrip, refStrip.getByText("12:30 PM", {exact: true}), isMobile);
      await captureWakeupComposers(page, reference, info.outputPath(`app-scheduled-${width}-${theme}.png`), info.outputPath(`reference-scheduled-${width}-${theme}.png`));
    }
  }
  await pauseWakeup(page, reference);
  for (const width of widths) {
    await resizeWakeupDemo(surfaces, width);
    for (const [row, composer] of [[appStrip, page.locator(".fleet-composer-surface")], [refStrip, reference.locator(".composer")]] as const) {
      await expectWakeupLine(row, row.getByText("Paused", {exact: true}), isMobile);
      expect(await composerSeam(row, composer)).toEqual(isMobile ? [8, 8, 8] : [20, 20, 8]);
    }
    await captureWakeupComposers(page, reference, info.outputPath(`app-paused-${width}.png`), info.outputPath(`reference-paused-${width}.png`));
  }
  await reference.close();
});

test("a future wake-up shows the complete date and time", async ({page, chatProbe, isMobile}, info) => {
  await page.clock.setFixedTime(new Date("2026-09-28T23:48:00Z"));
  await resizeWakeupDemo([page], isMobile ? 320 : 1440);
  await page.goto(`${chatProbe}?visual=1&state=wake-up`);
  const strip = page.getByRole("group", {name: "Every 30 minutes"}).locator("..");
  const time = strip.locator("time");
  await expect(time).toHaveText("Sep 29, 12:18 AM");
  await expectWakeupLine(strip, time, isMobile);
  await strip.screenshot({path: info.outputPath("app-future-wake-up.png"), animations: "disabled", scale: "css"});
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
  const a = await first.boundingBox(), b = await second.boundingBox();
  if (!a || !b) throw new Error("Both events must be visible to compare the gap");
  return Math.round(b.y - a.y - a.height);
}

async function composerSeam(strip: Locator, composer: Locator) {
  const a = await strip.boundingBox();
  const b = await composer.boundingBox();
  if (!a || !b) throw new Error("The wake-up strip and composer must both be visible");
  return [Math.round(a.x - b.x), Math.round(b.x + b.width - a.x - a.width), Math.round(b.y - a.y - a.height)];
}

async function expectWakeupLine(strip: Locator, status: Locator, isMobile: boolean) {
  await expect(status).toBeVisible();
  const box = await status.evaluate(node => {
    const range = document.createRange();
    range.selectNodeContents(node);
    const text = range.getBoundingClientRect();
    const label = node.parentElement?.getBoundingClientRect();
    if (!label) throw new Error("The wake-up label is missing");
    return {right: text.right, center: text.y + text.height / 2, overshoot: text.right - label.right};
  });
  expect(box.overshoot).toBeLessThanOrEqual(1);
  const buttons = await strip.getByRole("button").all();
  expect(buttons).toHaveLength(3);
  for (const button of buttons) {
    const action = await button.boundingBox();
    if (!action) throw new Error("A wake-up action has no visible bounds");
    expect(action.x).toBeGreaterThanOrEqual(box.right);
    expect(Math.abs(action.y + action.height / 2 - box.center)).toBeLessThanOrEqual(1);
    expect([action.width, action.height]).toEqual([28, isMobile ? 44 : 28]);
  }
}

async function captureWakeupComposers(page: Page, reference: Page, appPath: string, referencePath: string) {
  await page.getByTestId("fleet-chat-footer").screenshot({path: appPath, animations: "disabled", scale: "css"});
  await reference.locator(".composer-wrap").screenshot({path: referencePath, animations: "disabled", scale: "css"});
}

function resizeWakeupDemo(surfaces: Page[], width: number) {
  return Promise.all(surfaces.map(surface => surface.setViewportSize({width, height: width < 681 ? 844 : 1000})));
}

async function pauseWakeup(page: Page, reference: Page) {
  const schedule = {...visualWakeup(referenceTime.getTime()), source: "api", etag: '"sample"'};
  await page.route(`**/schedules/${schedule.schedule_id}`, route => {
    const change = route.request().method() === "PATCH" ? {status: "paused", run_state: "paused", next_run_at: null,
      override: {status: "paused", timezone: schedule.timezone}} : {};
    return route.fulfill({json: {...schedule, ...change}});
  });
  await page.getByRole("button", {name: "Pause wake-up"}).click();
  await reference.getByRole("button", {name: "Pause scheduled wake-up"}).click();
}
