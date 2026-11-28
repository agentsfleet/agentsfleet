import {expect} from "@playwright/test";
import {test} from "./fixtures/chat-probe";
import {RESEARCH} from "./fixtures/web-research-data";

test("a customer can follow a research question, both actions, and the cited answer in fleet chat", async ({page, chatProbe, isMobile}, info) => {
  // Exercise both supported citation styles across the two browser projects.
  await page.goto(`${chatProbe}?research=1${isMobile ? "&plain=1" : ""}`);
  const chat = page.getByRole("log", {name: "Chat"});
  const question = chat.locator('[data-role="user"]');
  const answer = chat.locator('[data-role="assistant"]');
  const [first] = RESEARCH.sources;
  await expect(question).toContainText(RESEARCH.question);
  await expect(answer.getByText(RESEARCH.answer, {exact: true})).toBeVisible();
  const citation = answer.getByRole("link", {name: isMobile ? first.url : first.title, exact: true});
  await expect(citation).toBeVisible();
  await expect(citation).toHaveAttribute("href", first.url);
  const group = answer.getByRole("button", {name: "Tool activity: Searched the web and read a page"});
  await group.click();
  const actions = answer.getByRole("list", {name: "Tool activity"});
  await expect(actions.getByRole("button", {name: /Searched the web/})).toContainText(RESEARCH.query);
  await expect(actions.getByRole("button", {name: /Read page/})).toContainText(first.url);
  await question.scrollIntoViewIfNeeded();
  await page.screenshot({path: info.outputPath("research-chat.png"), scale: "css", animations: "disabled"});
  if (isMobile) {
    await citation.scrollIntoViewIfNeeded();
    await expect(citation).toBeInViewport();
    await group.click();
    await citation.scrollIntoViewIfNeeded();
    await expect(citation).toBeInViewport();
    await page.screenshot({path: info.outputPath("research-answer.png"), scale: "css", animations: "disabled"});
  }

  await page.reload();
  await expect(question).toContainText(RESEARCH.question);
  await expect(answer.getByText(RESEARCH.answer, {exact: true})).toBeVisible();
  await expect(citation).toBeVisible();
  await expect(citation).toHaveAttribute("href", first.url);
  const panel = page.getByRole("region", {name: "Sources", exact: true});
  const sources = panel.getByRole("link");
  await expect(panel.getByRole("link", {name: first.title, exact: true})).toHaveAttribute("href", first.url);
  await expect(sources).toHaveCount(RESEARCH.sources.length);
  expect(await sources.evaluateAll(links => links.map(link => link.getAttribute("href")))).toEqual(RESEARCH.sources.map(source => source.url));
});
