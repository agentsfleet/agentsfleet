import { execFile } from "node:child_process";
import http from "node:http";
import path from "node:path";
import { promisify } from "node:util";
import { expect, test as base, type Locator, type Page } from "@playwright/test";
import { applicationCss } from "./fixtures/fleet-output-browser-assets";

const test = base.extend<object, {chatProbe: string}>({
  chatProbe: [async ({browserName}, provideProbe) => {
    const appRoot = path.resolve(import.meta.dirname, "../..");
    const built = await promisify(execFile)("bun", ["tests/e2e/fixtures/chat-browser-build.ts"],
      {cwd: appRoot, maxBuffer: 4_000_000, timeout: 20_000});
    const css = await applicationCss(appRoot);
    const server = http.createServer((request, response) => {
      if (request.url === "/probe.js") {
        response.writeHead(200, {"content-type": "text/javascript"}); response.end(built.stdout);
      } else {
        response.writeHead(200, {"content-type": "text/html"});
        response.end(`<html data-theme="dark"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><style>${css}</style></head><body><div id="root"></div><script type="module" src="/probe.js"></script></body></html>`);
      }
    });
    await new Promise<void>(resolve => server.listen(0, "127.0.0.1", resolve));
    const address = server.address();
    if (!address || typeof address === "string") throw new Error(`${browserName} chat fixture did not bind a TCP port`);
    try {await provideProbe(`http://127.0.0.1:${address.port}`);}
    finally {server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));}
  }, {scope: "worker"}],
});

test("conversation and composer use the shared 14px typography", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => {errors.push(error.message); console.error(error);});
  await page.goto(chatProbe);
  const answer = page.getByText("The request has an owner.", {exact: false});
  const user = page.getByText("Investigate the failing test and explain the evidence.", {exact: true});
  const input = page.getByRole("textbox");
  await expect(answer).toBeVisible();
  for (const text of [answer, user, input]) {
    await expect(text).toHaveCSS("font-size", "14px");
    await expect(text).toHaveCSS("line-height", "24.5px");
  }
  expect(await page.evaluate(async () => (await document.fonts.load('14px "Instrument Sans Variable"', "Typography")).length)).toBeGreaterThan(0);
  await input.fill("Keep the diagnosis concise and cite the source.");
  await expect(input).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
  const screenshot = info.outputPath("chat-14px-fixture.png");
  await page.screenshot({path: screenshot});
  await info.attach("chat-14px-fixture", {path: screenshot, contentType: "image/png"});
});

test("recorded actions stay visible and distinguish every terminal outcome", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?activity=1`);
  const group = page.getByRole("button", {name: /^Tool activity:/});
  await expect(group).toContainText("Opening pull request agentsfleet/linkwarden · 2 active");
  await expect(group).toHaveAccessibleName("Tool activity: Opening pull request agentsfleet/linkwarden · 2 active");
  await group.focus(); await page.keyboard.press("Enter");
  const tools = page.getByRole("list", {name: "Tool activity"});
  await expect(tools.getByText("Checked environment", {exact: true})).toBeVisible();
  await expect(tools.getByText("Read file", {exact: true})).toBeVisible();
  await expect(tools.getByText("Ran command", {exact: true})).toBeVisible();
  const pull = tools.locator('[data-tool="http_request"]').last();
  const action = pull.locator("button").first();
  await action.focus(); await page.keyboard.press("Enter");
  await expect(action).toHaveAttribute("aria-expanded", "true");
  await expect(pull.getByText("Input", {exact: true})).toBeVisible();
  await expectActivityGeometry(page, action, tools, info.project.name === "mobile-chromium");
  await page.emulateMedia({reducedMotion: "reduce"});
  for (const icon of await tools.locator('[data-tool-shimmer="true"]').all()) {
    await expect(icon).toHaveCSS("animation-name", "none");
    await expect(icon).toHaveCSS("color", "rgb(147, 197, 253)");
  }
  const screenshot = info.outputPath("chat-tool-activity-fixture.png");
  await page.screenshot({path: screenshot});
  await info.attach("chat-tool-activity-fixture", {path: screenshot, contentType: "image/png"});
  for (const [state, wording, color] of [["succeeded", "Opened pull request", "rgb(74, 222, 128)"], ["failed", "Failed: opening pull request", "rgb(252, 165, 165)"],
    ["interrupted", "Interrupted: opening pull request", "rgb(251, 191, 36)"], ["unconfirmed", "Outcome unavailable: opening pull request", "rgb(148, 160, 151)"]] as const) {
    await page.getByRole("button", {name: `Show ${state}`, exact: true}).click();
    await expect(pull.locator("[data-command-heading]")).toContainText(wording);
    await expect(pull.locator("[data-tool-bullet]")).toHaveCSS("color", color);
    if (state !== "succeeded") await expect(pull.locator("[data-command-heading]")).not.toContainText("Opened pull request");
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});

async function expectActivityGeometry(page: Page, action: Locator, tools: Locator, mobile: boolean) {
  for (const icon of await page.locator('[data-tool-bullet], [data-signal-spinner], [data-testid="fleet-message-sender"] svg').all()) {
    await expect(icon).toHaveCSS("width", "14px");
    await expect(icon).toHaveCSS("height", "14px");
  }
  await expect(action).toHaveCSS("outline-style", "solid");
  await expect(action).toHaveCSS("outline-width", "2px");
  await expect(action).toHaveCSS("outline-offset", "-2px");
  await expect(action).toHaveCSS("border-top-width", "0px");
  await expect(action).toHaveCSS("padding-left", "8px");
  await expect(action).toHaveCSS("padding-right", "8px");
  for (const button of await tools.locator("[data-command-heading]").all()) {
    expect((await button.locator("..").boundingBox())?.height).toBeGreaterThanOrEqual(mobile ? 44 : 36);
  }
  const heading = action.locator("[data-command-heading]");
  await expect(heading.getByText("Opening pull request", {exact: true})).toHaveCSS("font-size", "14px");
  const verb = await heading.getByText("Opening pull request", {exact: true}).boundingBox();
  const target = await heading.getByText("agentsfleet/linkwarden", {exact: true}).boundingBox();
  expect(verb).not.toBeNull(); expect(target).not.toBeNull();
  if (mobile) expect(target!.y).toBeGreaterThan(verb!.y);
  else expect(Math.abs(target!.y + target!.height / 2 - verb!.y - verb!.height / 2)).toBeLessThan(1);
}
