import type { Page } from "@playwright/test";
import { test, expect } from "@playwright/test";
import { DEFAULT_JOB, EXAMPLE_JOBS } from "../../src/lib/example-jobs";
import { createChatStory } from "../../src/lib/chat-story";

const MOTION_VIEWPORT_HEIGHT = 1000;
const MOTION_CLOCK_TIME = new Date("2026-10-14T00:00:00Z");
const MOTION_STEP_MS = 1000;
const HISTORY_SCROLL_DISTANCE = 120;
const INSTALL_COMMAND = "curl -fsSL https://agentsfleet.dev | bash";
const HOSTED_SIGN_UP = "https://app.agentsfleet.net/sign-up";
const INCIDENT_LINK = /Investigate an incident.*Explore example run/;
const BACK_LINK = "Back to home";
const BROWSER_TIMEOUT_DEFAULT = "Number(process.env.BROWSER_TIMEOUT) || 5";
const SAMPLE_TIMEOUT_LOG = "Browser has been open for more than 5 minutes.";
const INCIDENT_REQUEST_PREFIX = "Watch agentsfleet/linkwarden";
const CONTEXTUAL_EXAMPLES = [
  { job: EXAMPLE_JOBS[1], link: "Explore a test regression example" },
  { job: EXAMPLE_JOBS[2], link: "Explore a recurring work example" },
] as const;
test.beforeEach(async ({ page }) => { await page.emulateMedia({ reducedMotion: "reduce" }); await page.goto("/#how-it-works"); });

async function startMotion(page: Page) {
  await page.clock.install({ time: MOTION_CLOCK_TIME });
  await page.clock.pauseAt(MOTION_CLOCK_TIME.getTime() + MOTION_STEP_MS);
  await page.reload();
}

test("visitors inspect the full planned journey without connecting or running anything", async ({ page }) => {
  const mutations: string[] = [];
  page.on("request", request => { if (request.method() === "POST") mutations.push(request.url()); });
  await expect(page.getByRole("textbox")).toHaveCount(0);
  await expect(page.getByText(INSTALL_COMMAND, { exact: true })).toBeVisible();
  await expect(page.getByTestId("hero")).not.toContainText(/preview|release pending/i);
  await expect(page.getByRole("figure")).toContainText(DEFAULT_JOB.report);
  await expect(page.getByRole("figure")).toContainText("/schedule every 15 minutes");
  await expect(page.getByRole("figure")).toContainText(`${DEFAULT_JOB.name} created. Waiting for its next scheduled check.`);
  await expect(page.getByRole("figure")).not.toContainText(/Saved to your fleet library/);
  await expect(page.getByRole("link", { name: "Create fleet", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: "Replay" })).toHaveCount(0);
  await expect(page.getByRole("navigation", { name: "Example stages" })).toHaveCount(0);
  const citation = page.getByRole("button", { name: "E2 · GitHub" });
  await citation.scrollIntoViewIfNeeded();
  const pageScroll = await page.evaluate(() => scrollY);
  await citation.click();
  const evidence = page.getByRole("region", { name: "Sample evidence" });
  await expect(evidence).toContainText(BROWSER_TIMEOUT_DEFAULT);
  await expect(evidence).toBeInViewport();
  expect(await page.evaluate(() => scrollY)).toBe(pageScroll);
  await evidence.getByRole("button", { name: "Close evidence" }).click();
  await expect(citation).toBeInViewport();
  await expect(citation).toBeFocused();
  expect(mutations).toEqual([]);
});
test("signup cancellation preserves private text without sending or storing it", async ({ page, context }) => {
  await context.route(HOSTED_SIGN_UP, route => route.fulfill({ contentType: "text/html", body: "<p>Sign-up cancelled</p>" }));
  const request = "Keep my private investigation request";
  await page.goto("/");
  await page.getByRole("textbox").fill(request);
  await page.getByRole("link", { name: INCIDENT_LINK }).click();
  await expect(page.getByRole("textbox")).toHaveCount(0);
  await expect(page.getByRole("figure")).not.toContainText(request);
  const opened = page.waitForEvent("popup");
  await page.getByRole("link", { name: "Create fleet", exact: true }).click();
  const tab = await opened;
  await tab.waitForLoadState();
  expect(tab.url()).toBe(HOSTED_SIGN_UP);
  await tab.close();
  await expect(page.getByRole("figure")).toContainText(DEFAULT_JOB.report);
  expect(decodeURIComponent(page.url())).not.toContain(request);
  expect(await page.evaluate(() => JSON.stringify([sessionStorage, localStorage]))).not.toContain(request);
  await page.getByRole("link", { name: BACK_LINK, exact: true }).click();
  await expect(page.getByRole("textbox")).toHaveValue(request);
  await expect(page.getByRole("figure")).toHaveCount(0);
});
for (const destination of ["brand", "browser back"] as const) {
  test(`${destination} restores the home composer and its private draft`, async ({ page }) => {
    await page.goto("/");
    const request = "Preserve this draft on return home";
    await page.getByRole("textbox").fill(request);
    await page.getByRole("link", { name: INCIDENT_LINK }).click();
    await expect(page.getByRole("figure")).toBeVisible();
    if (destination === "brand") await page.getByTestId("brand-link").click();
    else await page.goBack();
    await expect(page).toHaveURL(/\/$/);
    await expect(page.getByRole("figure")).toHaveCount(0);
    await expect(page.getByRole("textbox")).toHaveValue(request);
  });
}
for (const job of EXAMPLE_JOBS) {
  test(`${job.label} starts its own animated conversation directly`, async ({ page }) => {
    await page.goto("/");
    await page.emulateMedia({ reducedMotion: "no-preference" });
    await startMotion(page);
    await page.getByRole("link", { name: new RegExp(job.label) }).click();
    const example = page.getByTestId("how-it-works");
    await example.scrollIntoViewIfNeeded();
    await expect(page.getByRole("textbox")).toHaveCount(0);
    await expect(example).toHaveAttribute("data-running", "true");
    await expect(page.getByRole("heading", { name: job.title, exact: true })).toBeVisible();
    const conversation = page.getByRole("figure");
    await expect(conversation).not.toContainText(job.report);
    await page.clock.runFor(createChatStory(job).duration - MOTION_STEP_MS);
    await expect(conversation).toContainText(job.report);
    await expect(conversation).toHaveAttribute("data-cycle", "0");
  });
}
test("autoplay types, appends, pauses, and loops without changing the page scroll", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: MOTION_VIEWPORT_HEIGHT });
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await startMotion(page);
  const example = page.getByTestId("how-it-works");
  await example.scrollIntoViewIfNeeded();
  await expect(example).toHaveAttribute("data-running", "true");
  await page.clock.runFor(MOTION_STEP_MS);
  const conversation = page.getByRole("figure");
  await expect(conversation).toContainText(INCIDENT_REQUEST_PREFIX);
  const bounds = await page.getByRole("log").boundingBox();
  if (!bounds) throw new Error("Missing example chat bounds");
  await page.mouse.move(bounds.x + bounds.width / 2, bounds.y + bounds.height / 2);
  await page.mouse.wheel(0, 12);
  await page.getByRole("log").dispatchEvent("touchstart");
  await expect(example).toHaveAttribute("data-running", "true");
  await page.getByRole("button", { name: "Pause", exact: true }).click();
  const paused = await conversation.textContent();
  await page.clock.runFor(5000); expect(await conversation.textContent()).toBe(paused);
  await page.getByRole("button", { name: "Play", exact: true }).click();
  const scroll = await page.evaluate(() => scrollY);
  await page.clock.runFor(createChatStory(DEFAULT_JOB).duration - 1100);
  await expect(conversation).toContainText(DEFAULT_JOB.report);
  await page.clock.runFor(300);
  await expect(conversation).toHaveAttribute("data-cycle", "1");
  await expect(conversation).not.toContainText(DEFAULT_JOB.report);
  expect(await page.evaluate(() => scrollY)).toBe(scroll);
});
test("keyboard evidence inspection keeps content and focus until explicit play", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await startMotion(page);
  await page.getByRole("button", { name: "Read full conversation" }).click();
  await expect(page.getByRole("button", { name: "Play", exact: true })).toBeFocused();
  const transcript = page.getByRole("log");
  const evidence = page.getByRole("button", { name: "E1 · Grafana Loki" });
  await page.getByRole("link", { name: BACK_LINK, exact: true }).focus();
  await page.keyboard.press("Tab");
  await expect(transcript).toBeFocused();
  await evidence.focus();
  await expect(evidence).toBeFocused();
  const pageScroll = await page.evaluate(() => scrollY);
  await page.keyboard.press("Home");
  await expect.poll(() => transcript.evaluate(element => element.scrollTop)).toBe(0);
  expect(await page.evaluate(() => scrollY)).toBe(pageScroll);
  await evidence.focus(); await page.keyboard.press("Enter");
  await page.clock.runFor(90000);
  await expect(evidence).toBeFocused();
  await expect(page.getByRole("region", { name: "Sample evidence" })).toContainText(SAMPLE_TIMEOUT_LOG);
  await expect(page.getByTestId("how-it-works")).toHaveAttribute("data-running", "false");
  await page.getByRole("button", { name: "Play", exact: true }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("region", { name: "Sample evidence" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Pause", exact: true })).toBeFocused();
  await expect(page.getByTestId("how-it-works")).toHaveAttribute("data-running", "true");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("button", { name: "Play", exact: true })).toBeFocused();
  await expect(page.getByTestId("how-it-works")).toHaveAttribute("data-running", "false");
});
test("scrolling through earlier messages keeps the animation running without dragging the reader to the bottom", async ({ page }) => {
  await page.setViewportSize({ width: 1440, height: MOTION_VIEWPORT_HEIGHT });
  await page.emulateMedia({ reducedMotion: "no-preference" });
  await startMotion(page);
  await page.getByTestId("how-it-works").scrollIntoViewIfNeeded();
  const step = createChatStory(DEFAULT_JOB).steps[8];
  if (!step) throw new Error("Missing example finding");
  await page.clock.runFor(step.endsAt);
  const conversation = page.getByRole("log");
  const position = await conversation.evaluate((element, distance) => {
    element.scrollTop -= distance;
    element.dispatchEvent(new Event("scroll", { bubbles: true }));
    return element.scrollTop;
  }, HISTORY_SCROLL_DISTANCE);
  await expect(page.getByRole("button", { name: "Jump to latest" })).toBeVisible();
  await page.clock.runFor(MOTION_STEP_MS);
  expect(await conversation.evaluate(element => element.scrollTop)).toBe(position);
  await expect(page.getByTestId("how-it-works")).toHaveAttribute("data-running", "true");
  await page.getByRole("button", { name: "Jump to latest" }).click();
  await page.clock.runFor(MOTION_STEP_MS);
  expect(await conversation.evaluate(element => element.scrollHeight - element.clientHeight - element.scrollTop)).toBeLessThanOrEqual(24);
  await expect(page.getByRole("button", { name: "Pause", exact: true })).toBeFocused();
});
for (const { job, link } of CONTEXTUAL_EXAMPLES) {
  test(`${job.label} opens its own example and restores the private draft on return`, async ({ page }) => {
    await page.goto("/");
    const composer = page.getByRole("textbox");
    const request = "Keep this request while exploring";
    await expect(composer).toHaveCount(1);
    await expect(page.getByText(INSTALL_COMMAND, { exact: true })).toBeVisible();
    await composer.fill(request);
    await page.getByRole("link", { name: new RegExp(job.label) }).click();
    await expect(composer).toHaveCount(0);
    await expect(page.getByRole("heading", { name: job.title, exact: true })).toBeVisible();
    await expect(page.getByRole("figure")).toContainText(job.report);
    await expect(page.getByRole("figure")).not.toContainText(DEFAULT_JOB.report);
    await expect(page.getByRole("figure")).not.toContainText(request);
    await expect(page.getByText(INSTALL_COMMAND, { exact: true })).toBeVisible();
    await page.getByRole("link", { name: BACK_LINK, exact: true }).click();
    await expect(page.getByTestId("how-it-works")).toHaveCount(0);
    await expect(composer).toHaveValue(request);
    await expect(page.getByRole("link", { name: link, exact: true })).toBeVisible();
  });
}
