import { expect } from "@playwright/test";
import { test } from "./fixtures/chat-probe";
import { LAUNCH_REPLIES } from "./fixtures/launch-review-activity";

const LABELS = ["Read 3 files", "Viewed 2 images", "UI reviewer started", "Edited files, ran 3 commands", "UI reviewer finished",
  "Used GitHub integration", "Started preview server", "Checked desktop and mobile layouts"];

test("Launch review interleaves eight keyboard disclosures with the answer and retains raw work", async ({page, chatProbe, isMobile}, info) => {
  if (!isMobile) await page.setViewportSize({width: 1440, height: 1024});
  await page.emulateMedia({reducedMotion: "reduce"});
  await page.goto(`${chatProbe}?visual=1&state=activity`);
  if (!isMobile) await expect(page.getByRole("navigation", {name: "Primary"}).getByRole("link", {name: "Launch review", exact: true})).toBeVisible();
  const chat = page.getByRole("log", {name: "Chat"});
  const reply = chat.locator('[data-role="assistant"]').last();
  const groups = reply.getByRole("list", {name: "Work activity"});
  await expect(groups).toHaveCount(2);
  const controls = groups.locator("summary");
  await expect(controls).toHaveText(LABELS);
  const visibleOrder = reply.locator('p, ul[aria-label="Work activity"] summary').filter({visible: true});
  const text = await visibleOrder.allTextContents();
  expect(text).toEqual(expect.arrayContaining([LAUNCH_REPLIES[0], LAUNCH_REPLIES[1], ...LABELS]));
  expect(text.indexOf(LAUNCH_REPLIES[0])).toBeLessThan(text.indexOf(LABELS[0]!));
  expect(text.indexOf(LAUNCH_REPLIES[1])).toBeGreaterThan(text.indexOf(LABELS[4]!));
  expect(text.indexOf(LAUNCH_REPLIES[1])).toBeLessThan(text.indexOf(LABELS[5]!));
  await expect(reply.getByRole("link", {name: "Launch preview"})).toHaveAttribute("href", "https://preview.example.test/launch");
  for (const name of ["launch-brief.md", "mobile-reference.png"]) await expect(chat.getByText(name, {exact: true})).toBeVisible();
  for (const control of await controls.all()) {
    const box = (await control.boundingBox())!;
    expect(box.height).toBeGreaterThanOrEqual(isMobile ? 44 : 36);
    await control.focus();
    await control.press("Enter");
    await expect(control.locator("..")).toHaveAttribute("open", "");
    await expect(control.locator("..").locator("p")).toBeVisible();
    await control.press("Space");
    await expect(control.locator("..")).not.toHaveAttribute("open", "");
  }
  await controls.last().blur();
  const worked = reply.getByRole("button", {name: /^Tool activity:/});
  await expect(worked).toHaveAttribute("aria-expanded", "false");
  await worked.click();
  await expect(reply.locator('[data-tool="wait_agent"]')).toHaveCount(1);
  const commands = reply.locator('[data-tool="exec_command"]');
  await expect(commands).toHaveCount(5);
  const commandLabel = commands.getByText("Ran command", {exact: true}).first();
  await expect(commandLabel).toHaveCSS("color", "rgb(133, 139, 149)");
  await expect(commandLabel).toHaveCSS("font-weight", "400");
  await worked.click();
  await reply.getByRole("button", {name: "Quote reply"}).click();
  const dismiss = page.getByRole("button", {name: "Remove quote"});
  for (const paragraph of LAUNCH_REPLIES) await expect(dismiss.locator("..")).toContainText(paragraph);
  await dismiss.click();
  for (const theme of ["dark", "light"]) {
    await page.evaluate(value => { document.documentElement.dataset.theme = value; }, theme);
    const commandSummary = groups.getByText("Edited files, ran 3 commands", {exact: true});
    const caption = theme === "dark" ? "rgb(133, 139, 149)" : "rgb(102, 112, 123)";
    await expect(commandSummary).toHaveCSS("color", caption);
    await expect(commandSummary).toHaveCSS("font-weight", "400");
    const agentIcon = controls.filter({hasText: "UI reviewer started"}).locator("svg").first();
    await expect(agentIcon).toHaveCSS("color", theme === "dark" ? "rgb(196, 180, 231)" : "rgb(106, 78, 139)");
    for (const [index, anchor] of (isMobile ? [reply.getByText(LAUNCH_REPLIES[0]), reply.getByText(LAUNCH_REPLIES[1])] : [reply.getByText(LAUNCH_REPLIES[0])]).entries()) {
      await anchor.evaluate(element => element.scrollIntoView({block: "start", behavior: "instant"}));
      await expect(anchor).toBeInViewport();
      for (const group of await groups.all()) expect(await group.evaluate(el => el.scrollWidth <= el.clientWidth)).toBe(true);
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
      const screenshot = info.outputPath(`launch-activity-${theme}-${index + 1}.png`);
      await page.screenshot({path: screenshot, scale: "css", animations: "disabled"});
      await info.attach(`launch-activity-${theme}-${index + 1}`, {path: screenshot, contentType: "image/png"});
    }
  }
});
