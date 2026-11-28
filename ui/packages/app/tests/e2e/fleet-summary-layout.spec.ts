import type { Locator } from "@playwright/test";
import { expect } from "@playwright/test";
import { DURABLE_TASK_IDS, detail, summary } from "@/tests/helpers/task-reads";
import { test } from "./fixtures/chat-probe";

test("Send stays in the conversation with a tall draft while reconnecting", async ({page, chatProbe}) => {
  await page.setViewportSize({width: 1280, height: 360});
  await page.goto(`${chatProbe}?visual=1&state=activity&reconnecting=1`);
  await expect(page.getByRole("button", {name: "Reconnecting…"})).toBeVisible();
  const composer = page.getByRole("form", {name: "Chat composer"});
  const input = composer.getByRole("textbox");
  const draft = Array.from({length: 30}, (_, line) => `draft line ${line + 1}`).join("\n");
  await input.fill(draft);
  await expect(input).toHaveValue(draft);
  const root = page.getByTestId("fleet-thread-root");
  const send = composer.getByRole("button", {name: "Send", exact: true});
  const frame = await root.boundingBox();
  const button = await send.boundingBox();
  expect(frame).not.toBeNull();
  expect(button).not.toBeNull();
  expect(button!.y + button!.height).toBeLessThanOrEqual(frame!.y + frame!.height + 1);
});

test("work stays in a sidebar while the ordinary chat composer preserves its draft", async ({page, chatProbe}, info) => {
  const work = [summary(), summary(DURABLE_TASK_IDS[1], "running"), summary(DURABLE_TASK_IDS[2], "ready")];
  await page.route("**/live/v1/workspaces/workspace/fleets/linkwarden/durable_tasks**", async route => {
    expect(route.request().headers()["authorization"]).toBeUndefined();
    const pathname = new URL(route.request().url()).pathname;
    const task = work.find(task => pathname.endsWith(task.id));
    const saved = task ? detail(task) : null;
    const target = task ? {kind: "existing_pr", repository: "linkwarden/linkwarden", number: Number(task.id.slice(-3)), branch: "repair", head: "a".repeat(40), tree: "b".repeat(40), url: `https://github.com/linkwarden/linkwarden/pull/${Number(task.id.slice(-3))}`, workflows: []} : null;
    await route.fulfill({headers: {etag: `"${"a".repeat(64)}"`}, json: saved ? {...saved, context: {...saved.context, target}} : {items: work, next_cursor: null}});
  });
  await page.goto(`${chatProbe}?durable_tasks=1`);
  const input = page.getByRole("textbox", {name: /^Message your fleet/});
  await input.fill("Keep the redirect fix in the callback handler.");
  for (const window of [{width: 1280, height: 800}, {width: 1280, height: 360}, {width: 390, height: 667}]) {
    await page.setViewportSize(window);
    await expect(page.getByRole("link", {name: /^Pull request #123 ·/})).toHaveAttribute("href", "https://github.com/linkwarden/linkwarden/pull/123");
    const panel = page.getByRole("region", {name: "Current work"});
    await expect(panel.getByRole("heading", {name: "Tasks", exact: true})).toBeVisible();
    for (const section of ["Pull requests", "Sources"]) await expect(panel.getByRole("heading", {name: section, exact: true})).toBeAttached();
    await expect(panel.getByRole("heading", {name: "Output", exact: true})).toHaveCount(0);
    await expect(panel.getByRole("heading", {name: "Changes", exact: true})).toHaveCount(0);
    await expect(panel.getByRole("region", {name: "Pull requests", exact: true})).toBeAttached();
    await expectSourceHierarchy(panel);
    if (window.height === 800) await page.screenshot({path: info.outputPath("context-sidebar-desktop.png"), scale: "css", animations: "disabled"});
    const repair = panel.getByRole("button", {name: /PR #123 repair/});
    if (await repair.getAttribute("aria-expanded") !== "true") {await repair.focus(); await page.keyboard.press("Enter");}
    const selectedDetail = panel.getByLabel("PR #123 repair details", {exact: true});
    await expect(selectedDetail.getByText("What counts as done?", {exact: true})).toBeVisible();
    await expect(selectedDetail.getByText("Repair the callback redirect without weakening the check.", {exact: true})).toBeVisible();
    expect(await panel.evaluate(node => !!node.closest(".ph-no-capture.ph-no-record"))).toBe(true);
    expect(await page.getByTestId("fleet-chat-footer").evaluate(node => !!node.querySelector('[aria-label="Current work"]'))).toBe(false);
    if (window.width < 1024) await page.keyboard.press("Escape");
    await expect(input).toHaveValue("Keep the redirect fix in the callback handler.");
    await expect(input).toHaveCSS("font-size", window.width <= 680 || info.project.name === "mobile-chromium" ? "16px" : "13px");
    await input.focus();
    await input.scrollIntoViewIfNeeded();
    const composer = await page.getByRole("form", {name: "Chat composer"}).boundingBox();
    expect(composer).not.toBeNull();
    expect(composer!.y + composer!.height).toBeLessThanOrEqual(window.height + 1);
    expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  }
  const screenshot = info.outputPath("fleet-sidebar-mobile.png");
  await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
  await info.attach("fleet-sidebar-mobile", {path: screenshot, contentType: "image/png"});
});

async function expectSourceHierarchy(panel: Locator) {
  const sourcesHeading = panel.getByRole("heading", {name: "Sources", exact: true});
  expect(await sourcesHeading.locator("svg").count()).toBe(0);
  const sources = panel.getByRole("region", {name: "Sources", exact: true});
  const disclosure = sources.locator("summary");
  const metrics = await disclosure.evaluate(node => {
    const heading = node.closest("section")!.querySelector("h2")!;
    return {heading: parseFloat(getComputedStyle(heading).fontSize), row: parseFloat(getComputedStyle(node).fontSize),
      icon: node.querySelector("svg")!.getBoundingClientRect().width,
      gap: node.getBoundingClientRect().top - heading.getBoundingClientRect().bottom};
  });
  expect(metrics.heading).toBe(13); expect(metrics.row).toBe(13);
  expect(metrics.icon).toBe(16);
  expect(metrics.gap).toBeGreaterThanOrEqual(4); expect(metrics.gap).toBeLessThanOrEqual(8);
  if (!(await sources.locator("details").evaluate(node => node.hasAttribute("open")))) await disclosure.click();
  await expect(sources.getByRole("link", {name: "linkwarden/linkwarden", exact: true})).toHaveAttribute("href", "https://github.com/linkwarden/linkwarden");
  await expect(sources.getByText("Policy: Edit existing PRs", {exact: true})).toBeVisible();
  await disclosure.click();
}

test("fleet Summary retains full inline phone content and room for policy labels", async ({page, chatProbe}, info) => {
  await page.setViewportSize(info.project.name === "chromium" ? {width: 1480, height: 1000} : {width: 390, height: 844});
  for (const theme of ["dark", "light"]) {
    await page.goto(chatProbe);
    await page.evaluate(value => {document.documentElement.dataset.theme = value;}, theme);
    const summary = page.getByRole("complementary", {name: "Summary", exact: true});
    await expect(summary).toBeVisible();
    const layout = await summary.evaluate(node => ({width: node.getBoundingClientRect().width,
      scroll: node.scrollHeight, client: node.clientHeight, maxHeight: getComputedStyle(node).maxHeight}));
    if (info.project.name === "chromium") expect(layout.width).toBeCloseTo(1480 * 0.23, 0);
    else {expect(layout.maxHeight).toBe("none"); expect(layout.scroll).toBeLessThanOrEqual(layout.client + 1);}
    const footer = page.getByTestId("fleet-chat-footer");
    expect(await footer.evaluate(node => {
      const viewport = node.closest('[role="presentation"]');
      if (!viewport) throw new Error("Native thread viewport is missing");
      return Math.abs(node.getBoundingClientRect().bottom - viewport.getBoundingClientRect().bottom);
    })).toBeLessThanOrEqual(1);
    await page.screenshot({path: info.outputPath(`matched-summary-${theme}.png`), scale: "css", animations: "disabled"});
  }
});
