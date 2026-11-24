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

test("repository Sources keeps each label on one line and opens with the keyboard", async ({page, chatProbe, isMobile}, info) => {
  const reference = await page.context().newPage();
  await Promise.all([
    page.goto(`${chatProbe}?visual=1&state=completed`),
    reference.goto(new URL("../../../../../docs/ux.html#conversation", import.meta.url).href),
  ]);
  const sources = page.getByRole("region", {name: "Sources", exact: true});
  const design = reference.locator(".summary-section").filter({has: reference.getByText("Sources", {exact: true})});
  const row = sources.locator("summary");
  const designRow = design.locator("summary");
  const repository = "agentsfleet/platform-images";
  for (const disclosure of [row, designRow]) {
    await expect(disclosure.locator("strong")).toHaveText(repository);
    await expect(disclosure.locator("small")).toHaveText("Policy: Edit existing PRs");
  }

  for (const theme of ["dark", "light"]) {
    for (const surface of [page, reference]) await surface.evaluate(async palette => {
      document.documentElement.dataset.theme = palette;
      await document.fonts.ready;
    }, theme);
    const expected = await sourceDisclosureGeometry(designRow);
    expect(expected.gaps).toEqual([8, 8]);
    expect(expected.text).toEqual({lineHeights: [20, 16], offset: [0, 0], policyFits: true});
    await expect.poll(() => sourceDisclosureGeometry(row)).toEqual(expected);
    if (!isMobile) expect((await row.boundingBox())?.height).toBe((await designRow.boundingBox())?.height);
    await sources.screenshot({path: info.outputPath(`app-sources-${theme}.png`), scale: "css", animations: "disabled"});
    await design.screenshot({path: info.outputPath(`reference-sources-${theme}.png`), scale: "css", animations: "disabled"});
  }

  if (!isMobile) {
    await page.setViewportSize({width: 768, height: 900});
    const compact = await sourceDisclosureGeometry(row);
    expect(compact.text).toEqual({lineHeights: [20, 16], offset: [0, 0], policyFits: true});
    await expect(row.locator("strong")).toHaveCSS("text-overflow", "ellipsis");
    expect(await row.locator("strong").evaluate(element => element.scrollWidth > element.clientWidth)).toBe(true);
    await sources.screenshot({path: info.outputPath("app-sources-narrow.png"), scale: "css", animations: "disabled"});
  }

  const link = sources.getByRole("link", {name: repository, exact: true});
  await expect(link).toBeHidden();
  await row.press("Enter");
  await expect(link).toBeVisible();
  await expect(link).toHaveAttribute("href", `https://github.com/${repository}`);
  await row.press("Space");
  await expect(link).toBeHidden();
  await reference.close();
});

function sourceDisclosureGeometry(row: Locator) {
  return row.evaluate(node => {
    const [icon, copy, caret] = Array.from(node.children, child => child.getBoundingClientRect());
    const title = node.querySelector("strong");
    const policy = node.querySelector("small");
    if (!icon || !copy || !caret || !title || !policy) throw new Error("The Sources disclosure is missing its icon, labels, or caret");
    const titleBox = title.getBoundingClientRect();
    const policyBox = policy.getBoundingClientRect();
    const rowStyle = getComputedStyle(node);
    return {
      gaps: [copy.left - icon.right, caret.left - copy.right],
      icons: [icon.width, caret.width],
      padding: [rowStyle.paddingTop, rowStyle.paddingBottom],
      radius: rowStyle.borderTopLeftRadius,
      text: {
        lineHeights: [titleBox.height, policyBox.height],
        offset: [policyBox.left - titleBox.left, policyBox.top - titleBox.bottom],
        policyFits: policy.scrollWidth <= policy.clientWidth,
      },
      typography: [title, policy].map(element => {
        const style = getComputedStyle(element);
        return [style.fontSize, style.lineHeight, style.fontWeight, style.color];
      }),
    };
  });
}

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

test("long repository names reveal their ending without moving icons or counts", async ({page, chatProbe, isMobile}, info) => {
  const repository = "agentsfleet/platform-images-with-a-long-workspace-repository-name";
  await page.emulateMedia({reducedMotion: "no-preference"});
  await page.goto(chatProbe + "?visual=1&state=activity&repositoryLabel=" + encodeURIComponent(repository));
  const row = page.getByRole("button", {name: new RegExp(repository)});
  const viewport = row.locator(".repo-name");
  const label = viewport.locator("span");
  const bounds = () => row.evaluate(node => ({
    icon: node.querySelector("svg")!.getBoundingClientRect().x,
    count: node.querySelector('[aria-label*="addition"]')?.getBoundingClientRect().x ?? node.lastElementChild!.getBoundingClientRect().x,
  }));
  const pinned = await bounds();
  if (!isMobile) {
    const visualOffset = () => label.evaluate(el => Math.round(el.getBoundingClientRect().left - el.parentElement!.getBoundingClientRect().left));
    const overflow = await label.evaluate(el => Math.round(el.getBoundingClientRect().width - el.parentElement!.getBoundingClientRect().width));
    expect(overflow).toBeGreaterThan(40);
    await row.hover();
    await expect.poll(visualOffset).toBe(-overflow);
    await page.screenshot({path:info.outputPath("repository-name-revealed.png"),animations:"allow"});
    await page.mouse.move(0,0);
    await expect.poll(visualOffset).toBe(0);
    await page.keyboard.press("Tab");
    await row.focus();
    await expect.poll(visualOffset).toBe(-overflow);
    expect(await bounds()).toEqual(pinned);
  }
  await page.emulateMedia({reducedMotion:"reduce"});
  await expect(label).toHaveCSS("white-space","normal");
  await expect(label).toHaveCSS("left","0px");
  expect(await label.evaluate(el => el.scrollWidth <= el.clientWidth + 1)).toBe(true);
  await row.click();
  await expect(page.getByRole("dialog")).toBeVisible();
});
