import {expect, type Locator} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

test("the launch review Summary shows the recorded output, people, processes and attached sources", async ({page, chatProbe}, info) => {
  await page.goto(chatProbe + "?visual=1&state=activity");
  const summary = page.getByRole("complementary", {name: "Summary"});
  const outputs = summary.getByRole("region", {name: "Output", exact: true});
  const workers = summary.getByRole("region", {name: "Agents & processes"});
  const sources = summary.getByRole("region", {name: "Sources", exact: true});
  await expect(outputs.getByRole("link", {name: "Launch preview"})).toHaveAttribute("href", "https://preview.example.test/launch");
  await expect(workers.locator("summary").filter({hasText:"UI reviewer"})).toContainText("Done");
  const server = workers.locator("summary").filter({hasText:"Preview server"});
  await expect(server).toContainText("Was running");
  for (const theme of ["dark","light"]) {
    await page.evaluate(value => { document.documentElement.dataset.theme = value; }, theme);
    await expect(summary).toHaveCSS("background-color", theme === "dark" ? "rgb(32, 35, 40)" : "rgb(240, 242, 245)");
    await summary.scrollIntoViewIfNeeded();
    const path=info.outputPath("launch-summary-" + theme + ".png");
    await summary.screenshot({path, animations:"disabled"});
    await info.attach("launch-summary-" + theme, {path,contentType:"image/png"});
  }
  await server.focus();
  await page.keyboard.press("Enter");
  await expect(workers.getByText(/Process running with session ID 12/)).toBeVisible();
  await page.keyboard.press("Space");
  await expect(workers.getByText(/Process running with session ID 12/)).toBeHidden();
  const attachments = sources.locator("summary").filter({hasText:"2 attached files"});
  await attachments.focus();
  await page.keyboard.press("Enter");
  await expect(sources.getByText("launch-brief.md")).toBeVisible();
  await expect(sources.getByText("mobile-reference.png")).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
});

test("a current preview process has a running status and both repositories agree with the changes flap", async ({page, chatProbe}, info) => {
  await page.goto(chatProbe + "?visual=1&state=activity&openSession=1");
  const summary = page.getByRole("complementary", {name: "Summary"});
  const changes = summary.getByRole("region", {name: "Changes"});
  await expect(changes.getByRole("button").filter({hasText: "web"})).toContainText("+248");
  await expect(changes.getByRole("button").filter({hasText: "web"})).toContainText("−3");
  await expect(changes.getByRole("button").filter({hasText: "docs"})).toContainText("+23");
  await expect(summary.locator("summary").filter({hasText: "Preview server"})).toHaveText(/Preview serverRunning/);
  const flap = page.getByRole("button").filter({hasText: "6 files changed"});
  await expect(flap).toContainText("+271");
  await expect(flap).toContainText("−3");
  for (const theme of ["dark","light"]) {
    await page.evaluate(value => { document.documentElement.dataset.theme = value; }, theme);
    await expect(summary).toHaveCSS("background-color", theme === "dark" ? "rgb(32, 35, 40)" : "rgb(240, 242, 245)");
    await summary.scrollIntoViewIfNeeded();
    await summary.screenshot({path:info.outputPath("open-session-summary-" + theme + ".png"), animations:"disabled"});
  }
});

test("Summary geometry and colors match ux.html on desktop and mobile in both themes", async ({page, chatProbe}) => {
  await page.goto(chatProbe + "?visual=1&state=activity");
  const summary = page.getByRole("complementary", {name: "Summary"});
  const section = summary.getByRole("heading", {name: "Agents & processes"});
  await expect(section).toBeVisible();
  const reference = await page.context().newPage();
  await reference.goto(new URL("../../../../../docs/ux.html#activity", import.meta.url).href);
  const design = reference.locator("aside.summary");
  await expect.poll(() => rowPositions(summary, "h2,button,a,summary")).toEqual(await rowPositions(design, "h2,.summary-label,.summary-row"));
  for (const appearance of ["dark", "light"]) {
    for (const surface of [page, reference]) await surface.evaluate(theme => { document.documentElement.dataset.theme = theme; }, appearance);
    const pairs = [[summary, design], [summary.locator("h2").first(), design.locator("h2").first()],
      [section, design.getByText("Agents & processes", {exact: true})]] as const;
    for (const [actual, expected] of pairs) {
      await expect.poll(() => geometry(actual)).toEqual(await geometry(expected));
    }
  }
  await reference.close();
});

function geometry(element: Locator) {
  return element.evaluate(node => {
    const css = getComputedStyle(node);
    return {font: css.fontSize, lineHeight: css.lineHeight, weight: css.fontWeight, ink: css.color, background: css.backgroundColor,
      radius: css.borderTopLeftRadius, padding: css.paddingLeft, width: Math.round(node.getBoundingClientRect().width)};
  });
}


function rowPositions(panel: Locator, selectors: string) {
  return panel.evaluate((node, query) => {
    const top = node.getBoundingClientRect().top;
    return Array.from(node.querySelectorAll(query), child => {
      const box = child.getBoundingClientRect();
      return [Math.round(box.y - top), Math.round(box.height)];
    });
  }, selectors);
}
