import { test } from "./fixtures/chat-probe";
import { expect, type Locator, type Page } from "@playwright/test";

test("the chat row uses the design-system stream motion token", async ({page, chatProbe}) => {
  await page.emulateMedia({reducedMotion: "no-preference"});
  await page.goto(chatProbe);
  const reply = page.locator('[data-role="assistant"]').first();
  await expect(reply).toHaveCSS("transition-duration", "0.08s");
  await page.evaluate(() => document.documentElement.style.setProperty("--motion-duration-stream", "200ms"));
  await expect(reply).toHaveCSS("transition-duration", "0.2s");
});

test("fleet status text has a muted-to-foreground sweep with accessible motion fallbacks", async ({page, chatProbe}) => {
  for (const route of ["?loading=thinking", "?activity=1"]) {
    await page.emulateMedia({reducedMotion: "no-preference", forcedColors: "none"});
    await page.goto(`${chatProbe}${route}`);
    const label = page.locator('[data-chat-loading="true"]').first();
    await expect(label).toBeVisible();
    await expect(label).toHaveCSS("animation-name", "chat-status-sweep");
    await expect(label).toHaveCSS("animation-duration", "2.8s");
    await expect(label).toHaveCSS("animation-timing-function", "cubic-bezier(0.4, 0, 0.2, 1)");
    await expect(label).toHaveCSS("opacity", "1");
    if (route === "?activity=1") {
      const activity = page.getByRole("button", {name: /^Tool activity:/}).filter({has: page.locator('[data-chat-loading="true"]')}).first();
      await expect(activity.locator(".page-loading-stroke")).toHaveCount(0);
      await expect(activity.locator("svg").first()).toHaveCSS("animation-name", "none");
    }
    for (const theme of ["light", "dark"]) {
      await page.locator("html").evaluate((node, value) => node.setAttribute("data-theme", value), theme);
      const gradient = await label.evaluate(node => getComputedStyle(node).backgroundImage);
      expect(gradient).toContain(theme === "dark" ? "rgb(163, 171, 183)" : "rgb(94, 104, 118)");
      expect(gradient).toContain(theme === "dark" ? "rgb(245, 246, 248)" : "rgb(32, 36, 43)");
    }
    const before = await label.boundingBox();
    const position = await label.evaluate(element => getComputedStyle(element).backgroundPosition);
    await expect.poll(() => label.evaluate(element => getComputedStyle(element).backgroundPosition)).not.toBe(position);
    expect(await label.boundingBox()).toEqual(before);
    await page.emulateMedia({reducedMotion: "reduce"});
    await expect(label).toHaveCSS("animation-name", "none");
    await expect(label).toHaveCSS("background-image", "none");
    await expect(label).not.toHaveCSS("-webkit-text-fill-color", "rgba(0, 0, 0, 0)");
    await page.emulateMedia({reducedMotion: "no-preference", forcedColors: "active"});
    await expect(label).toHaveCSS("animation-name", "none");
    await expect(label).toHaveCSS("background-image", "none");
    await expect(label).not.toHaveCSS("-webkit-text-fill-color", "rgba(0, 0, 0, 0)");
    if (route === "?activity=1") {
      await page.getByRole("button", {name: "Show succeeded", exact: true}).click();
      await expect(page.locator('[data-chat-loading="true"]')).toHaveCount(0);
    }
  }
});

test("incoming progress never moves the draft or the reading position, and Jump to latest follows it", async ({page, chatProbe}) => {
  await page.goto(`${chatProbe}?scroll=1`);
  const input = page.getByRole("textbox");
  const viewport = page.locator('[data-testid="fleet-thread-root"] [role="presentation"]');
  const updates = page.getByText("New recorded update: the callback regression is still running. Its result has not been confirmed.", {exact: true});
  await expect(input).toBeVisible();
  await page.evaluate(() => document.fonts.ready);
  const draft = "Keep the login fix in the callback handler. Preserve session expiry.";
  // Match Claude: a live update never moves the view, at the bottom or not, composing or not.
  for (const {atBottom, composing} of [{atBottom: true, composing: false}, {atBottom: true, composing: true}, {atBottom: false, composing: true}, {atBottom: false, composing: false}]) {
    await input.fill(composing ? draft : "");
    if (!composing) await input.blur();
    await expect.poll(() => viewport.evaluate(element => element.scrollHeight > element.clientHeight)).toBe(true);
    if (atBottom) {
      await viewport.evaluate(element => {element.scrollTop = element.scrollHeight;});
      await expect.poll(() => viewport.evaluate(element => element.scrollHeight - element.clientHeight - element.scrollTop)).toBeLessThanOrEqual(1);
    } else {
      // Let the reader's gesture cancel pending follow before progress arrives.
      await viewport.hover();
      await page.mouse.wheel(0, -100_000);
      await expect.poll(() => viewport.evaluate(element => element.scrollTop)).toBeLessThanOrEqual(1);
      await expect(page.getByRole("button", {name: "Jump to latest"})).toBeVisible();
    }
    const before = await updates.count();
    const samples = await viewport.evaluate(element => new Promise<number[]>(resolve => {
      const positions = [element.scrollTop];
      window.dispatchEvent(new Event("fixture:progress"));
      const sample = () => {
        positions.push(element.scrollTop);
        if (positions.length === 20) resolve(positions);
        else requestAnimationFrame(sample);
      };
      requestAnimationFrame(sample);
    }));
    expect(Math.max(...samples) - Math.min(...samples), `atBottom=${atBottom}, composing=${composing}`).toBeLessThanOrEqual(1);
    await expect(updates).toHaveCount(before + 1);
    expect(Math.abs(await viewport.evaluate(element => element.scrollTop) - samples[0]!)).toBeLessThanOrEqual(1);
    await expect(input).toHaveValue(composing ? draft : "");
    if (composing) await expect(input).toBeFocused();
    else await expect(input).not.toBeFocused();
  }
  // Match Claude: the reader follows the progress with Jump to latest; the first case above shows the view held there.
  await page.getByRole("button", {name: "Jump to latest"}).click();
  await expect.poll(() => viewport.evaluate(element => element.scrollHeight - element.clientHeight - element.scrollTop)).toBeLessThanOrEqual(1);
  await expect(input).toHaveValue("");
});

// The composer's own growth is measured over a live reply in fleet-thread.spec.ts.
test("opens at the latest turn, lifts a sent turn to the top and holds it while the reply streams and the window resizes", async ({page, chatProbe}) => {
  const roomy = page.viewportSize();
  if (roomy === null) throw new Error("the project sets no viewport to resize from");
  await page.addInitScript(() => {
    const nativeObserver = ResizeObserver;
    const mark = (name: string) => performance.mark(`chat-opening:${name}`);
    window.ResizeObserver = class extends nativeObserver {
      private observesLog = false;
      constructor(callback: ResizeObserverCallback) {
        super((entries, observer) => {
          for (const entry of entries) {
            if (entry.target.classList.contains("fleet-thread-log")) mark(`resize ${entry.contentRect.height}`);
          }
          callback(entries, observer);
        });
      }
      observe(target: Element, options?: ResizeObserverOptions): void {
        if (target.classList.contains("fleet-thread-log")) this.observesLog = true;
        super.observe(target, options);
      }
      disconnect(): void {
        if (this.observesLog) mark(`disconnect ${new Error().stack?.split("\n").slice(2, 4).join(" ")}`);
        super.disconnect();
      }
    };
    const scrollTo = Reflect.get(Element.prototype, "scrollTo") as Element["scrollTo"];
    Element.prototype.scrollTo = new Proxy(scrollTo, {
      apply(scrollTo, element, args) {
        if (element instanceof Element && element.classList.contains("fleet-thread-viewport")) {
          mark(`scrollTo ${args[0]?.top ?? args[1]} ${args[0]?.behavior ?? "auto"}`);
        }
        return Reflect.apply(scrollTo, element, args);
      },
    });
    window.addEventListener("scroll", event => {
      if (event.target instanceof Element && event.target.classList.contains("fleet-thread-viewport")) mark("scroll");
    }, {capture: true, passive: true});
  });
  // A settled reply taller than its layout placeholder sits just above the newest turn.
  await page.goto(`${chatProbe}?scroll=1&tall=18`);
  const viewport = page.locator('[data-testid="fleet-thread-root"] [role="presentation"]');
  const input = page.getByRole("textbox");
  await expect(input).toBeVisible();
  const fromBottom = () => viewport.evaluate(element => element.scrollHeight - element.clientHeight - element.scrollTop);
  const scrollTop = () => viewport.evaluate(element => element.scrollTop);
  const painted = () => page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
  try {
    await expect.poll(fromBottom).toBeLessThanOrEqual(1);
  } catch (cause) {
    const timeline = await page.evaluate(() => performance.getEntriesByType("mark")
      .filter(mark => mark.name.startsWith("chat-opening:"))
      .map(mark => `${Math.round(mark.startTime)}ms ${mark.name}`));
    throw new Error(`Opening scroll timeline\n${timeline.join("\n")}`, {cause});
  }
  await input.fill("Check the canary rollout.");
  await page.evaluate(() => performance.clearMarks());
  await page.getByRole("button", {name: "Send", exact: true}).click();
  const sent = page.locator("[data-aui-top-anchor-user]");
  await expect(sent).toContainText("Check the canary rollout.");
  // NaN, failing every bound, if the sent turn is no longer the anchor.
  const sentTop = () => viewport.evaluate(element => {
    const anchor = element.querySelector("[data-aui-top-anchor-user]");
    return anchor === null ? Number.NaN : anchor.getBoundingClientRect().top - element.getBoundingClientRect().top;
  });
  await expect.poll(async () => Math.abs(await sentTop())).toBeLessThanOrEqual(1);
  const sendScrolls = await page.evaluate(() => performance.getEntriesByType("mark")
    .map(mark => mark.name)
    .filter(name => name.startsWith("chat-opening:scrollTo"))
    .map(name => name.split(" ").at(-1)));
  expect(sendScrolls).toContain("instant");
  expect(sendScrolls).not.toContain("smooth");
  // The reserve is the room below a short reply that lets the sent turn reach the top.
  expect(await page.locator("[data-aui-top-anchor-reserve]").evaluate(element => element.getBoundingClientRect().height)).toBeGreaterThan(0);
  const held = await scrollTop();
  await page.evaluate(async chunks => {
    for (let chunk = 0; chunk < chunks; chunk++) {
      window.dispatchEvent(new Event("fixture:chunk"));
      await new Promise(resolve => requestAnimationFrame(resolve));
    }
  }, 25);
  // Match Claude: the reply runs on below the composer and the view never follows it.
  await expect.poll(fromBottom).toBeGreaterThan(1);
  expect(Math.abs(await scrollTop() - held)).toBeLessThanOrEqual(1);
  await expect(page.getByRole("button", {name: "Jump to latest"})).toBeVisible();
  // The composer stays on the chat's floor and inside the window over the reply,
  // at phone width too (owner, Nov 16, 2026). NaN fails the bound if either box is gone.
  const composerOffFloor = () => page.evaluate(() => {
    const root = document.querySelector('[data-testid="fleet-thread-root"]')?.getBoundingClientRect();
    const footer = document.querySelector('[data-testid="fleet-chat-footer"]')?.getBoundingClientRect();
    if (!root || !footer) return Number.NaN;
    return Math.max(Math.abs(root.bottom - footer.bottom), footer.bottom - window.innerHeight);
  });
  expect(await composerOffFloor()).toBeLessThanOrEqual(1);
  for (const size of [{width: roomy.width, height: 360}, roomy]) {
    await page.setViewportSize(size);
    await painted();
    expect(Math.abs(await scrollTop() - held)).toBeLessThanOrEqual(1);
    expect(Math.abs(await sentTop())).toBeLessThanOrEqual(1);
    expect(await composerOffFloor()).toBeLessThanOrEqual(1);
  }
  await page.getByRole("button", {name: "Jump to latest"}).click();
  await expect.poll(fromBottom).toBeLessThanOrEqual(1);
});

test("conversation and composer use the approved Manrope scale", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => {errors.push(error.message); console.error(error);});
  await page.goto(chatProbe);
  const answer = page.getByText("The request has an owner.", {exact: false});
  const user = page.getByText("Investigate the failing test and explain the evidence.", {exact: true});
  const input = page.getByRole("textbox");
  await expect(answer).toBeVisible();
  const mobile = info.project.name === "mobile-chromium";
  for (const text of [answer, user, input]) {
    await expect(text).toHaveCSS("font-size", mobile ? "16px" : "13px");
    await expect(text).toHaveCSS("line-height", mobile ? text === input ? "24px" : "26px" : "20px");
    await expect(text).toHaveCSS("font-weight", "400");
  }
  expect(await page.evaluate(async () => (await document.fonts.load('13px "Manrope Variable"', "Typography")).length)).toBeGreaterThan(0);
  await input.fill("Keep the diagnosis concise and cite the source.");
  await expect(input).toBeFocused();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
  const screenshot = info.outputPath("chat-manrope-fixture.png");
  await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
  await info.attach("chat-manrope-fixture", {path: screenshot, contentType: "image/png"});
});

test("incoming messages and failed requests have readable details", async ({page, chatProbe}, info) => {
  await page.emulateMedia({reducedMotion: "reduce"});
  await page.goto(`${chatProbe}?messages=1`);
  await expect(page.getByText("Message received", {exact: true})).toBeVisible();
  const disclosure = page.getByRole("button", {name: "View message"});
  await disclosure.focus(); await page.keyboard.press("Enter");
  await expect(page.getByText("Diagnose the failed title regression using the current source.")).toBeVisible();
  await expect(page.locator("pre")).toHaveCount(0);
  const failure = page.getByRole("alert").filter({hasText: "The response was interrupted."});
  await expect(failure).toBeVisible();
  await expect(failure).toHaveCSS("font-size", "13px");
  const details = failure.getByRole("button", {name: "View details"});
  await expect(page.getByText("The connection to the model provider was lost mid-turn.")).not.toBeVisible();
  await details.focus(); await page.keyboard.press("Enter");
  await expect(page.getByText("The connection to the model provider was lost mid-turn.")).toBeVisible();
  await page.keyboard.press("Enter");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  const screenshot = info.outputPath("message-clarity.png");
  await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
  await info.attach("message-clarity", {path: screenshot, contentType: "image/png"});
});

test("recorded actions stay visible and distinguish every terminal outcome", async ({page, chatProbe}, info) => {
  const errors: string[] = []; page.on("pageerror", error => errors.push(error.message));
  await page.goto(`${chatProbe}?activity=1`);
  const group = page.getByRole("button", {name: /^Tool activity:/});
  await expect(group).toContainText("Opening pull request");
  await expect(group).not.toContainText("agentsfleet/linkwarden");
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
  await expect(tools.locator('[data-tool-shimmer="true"]')).toHaveCount(0);
  const runningLabels = tools.locator('[data-chat-loading="true"]');
  expect(await runningLabels.count()).toBeGreaterThan(0);
  for (const label of await runningLabels.all()) {
    await expect(label).toHaveCSS("animation-name", "none");
    await expect(label).toHaveCSS("background-image", "none");
  }
  const screenshot = info.outputPath("chat-tool-activity-fixture.png");
  await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
  await info.attach("chat-tool-activity-fixture", {path: screenshot, contentType: "image/png"});
  for (const [state, wording, color] of [["succeeded", "Opened pull request", "rgb(194, 199, 208)"], ["failed", "Failed: opening pull request", "rgb(255, 179, 182)"],
    ["interrupted", "Interrupted: opening pull request", "rgb(243, 203, 135)"], ["unconfirmed", "Outcome unavailable: opening pull request", "rgb(163, 171, 183)"]] as const) {
    await page.getByRole("button", {name: `Show ${state}`, exact: true}).click();
    await expect(pull.locator("[data-command-heading]")).toContainText(wording);
    await expect(pull.locator("[data-tool-bullet]")).toHaveCSS("color", color);
    if (state !== "succeeded") await expect(pull.locator("[data-command-heading]")).not.toContainText("Opened pull request");
  }
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
  expect(errors).toEqual([]);
});

async function expectActivityGeometry(page: Page, action: Locator, tools: Locator, mobile: boolean) {
  for (const icon of await page.locator('[data-tool-bullet], [role="status"] > svg:has(.page-loading-stroke)').all()) {
    await expect(icon).toHaveCSS("width", "16px");
    await expect(icon).toHaveCSS("height", "16px");
  }
  for (const logo of await page.locator('[data-testid="fleet-message-sender"] svg').all()) {
    await expect(logo).toHaveCSS("width", "18px");
    await expect(logo).toHaveCSS("height", "18px");
    await expect(logo).toHaveCSS("padding", "0px");
  }
  await expect(action).toHaveCSS("--tw-ring-offset-width", "4px");
  await expect(action).toHaveCSS("box-shadow", /0px 0px 0px 6px/);
  await expect(action).toHaveCSS("border-top-width", "0px");
  await expect(action).toHaveCSS("padding-left", "8px");
  await expect(action).toHaveCSS("padding-right", "8px");
  for (const button of await tools.locator("[data-command-heading]").all()) {
    expect((await button.locator("..").boundingBox())?.height).toBeGreaterThanOrEqual(mobile ? 44 : 36);
  }
  const heading = action.locator("[data-command-heading]");
  await expect(heading.getByText("Opening pull request", {exact: true})).toHaveCSS("font-size", "13px");
  await expect(heading.getByText("agentsfleet/linkwarden", {exact: true})).toHaveCount(0);
  if (await action.getAttribute("aria-expanded") !== "true") await action.click();
  const row = action.locator("xpath=ancestor::li[@data-tool]");
  await expect(row.locator("pre").filter({hasText: "agentsfleet/linkwarden"}).first()).toBeVisible();
  await expect(row.locator("[data-tool-duration]")).toBeVisible();

}
