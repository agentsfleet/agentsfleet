import {expect, test} from "@playwright/test";

const review = (view: string) => new URL(`../../../../../docs/ux.html#${view}`, import.meta.url).href;

test("the populated navigation lists fleet chats and keeps credits in the footer", async ({page, isMobile}) => {
  test.skip(isMobile, "The mobile sidebar uses an off-canvas fleet list");
  await page.setViewportSize({width: 1120, height: 660});
  await page.goto(review("chief"));
  const nav = page.getByRole("navigation", {name: "Main navigation"});
  await expect(nav.getByRole("button", {name: "Chief of Staff", exact: true})).toBeVisible();
  await expect(nav.getByRole("button", {name: /Edit Chief of Staff/})).toHaveCount(0);
  await expect(nav.getByRole("button", {name: /Credits.*\$5\.00/})).toHaveCSS("border-top-width", "0px");
  await expect(page.getByRole("link", {name: /tightening review/i})).toHaveCount(0);
  await expect(page.getByRole("button", {name: "Model proposal"})).toHaveCount(0);
  expect(await page.locator(".review-links button[data-view]").evaluateAll(buttons =>
    buttons.slice(0, 3).map(button => button.textContent))).toEqual(["App loading", "New fleet", "Fleet handoff"]);
  const geometry = await nav.evaluate(rail => {
    const {right, bottom} = rail.getBoundingClientRect();
    const footer = rail.querySelector(".rail-bottom")!.getBoundingClientRect();
    const edges = Array.from(rail.querySelectorAll(".fleet-row"), row => row.getBoundingClientRect().right);
    return {right, bottom, footerBottom: footer.bottom, edges};
  });
  expect(geometry.edges.length).toBeGreaterThan(4);
  expect(new Set(geometry.edges).size).toBe(1);
  expect(geometry.edges[0]).toBeLessThan(geometry.right);
  expect(geometry.footerBottom).toBeLessThan(geometry.bottom);
});

test("the reference opens a fleet chat from its first message", async ({page}) => {
  await page.emulateMedia({reducedMotion: "reduce"});
  await page.goto(review("empty"));
  await expect(page.locator(".empty-heading h2")).toHaveText("New fleet");
  await page.locator("#message").fill("Check the latest deploy.");
  await page.getByRole("button", {name: "Send message", exact: true}).click();
  await expect(page).toHaveURL(/#fleet-1$/);
  await expect(page.locator("#page-title")).toHaveText("Fleet 1");
  await expect(page.getByRole("region", {name: "Fleet conversation"}).getByText("Check the latest deploy.", {exact: true})).toBeInViewport();
  await expect(page.getByRole("region", {name: "Wakes on"})).toContainText("Only a message");
  await expect(page.getByRole("region", {name: "Can access"})).toContainText("No access approved yet");
  await expect(page.getByRole("region", {name: "Remembers"})).toContainText("Nothing pinned yet");
  await expect(page.getByRole("switch", {name: "Allow all"})).not.toBeChecked();
  await expect(page.getByRole("switch", {name: "Packages"})).not.toBeChecked();
});

test("the reference keeps a refused first message and its model for Resend", async ({page}) => {
  await page.emulateMedia({reducedMotion: "reduce"});
  const url = new URL(review("empty"));
  url.searchParams.set("refuse-first", "");
  await page.goto(url.href);
  const model = page.getByRole("combobox", {name: "Fleet model"});
  await expect(model).toHaveValue("");
  await model.selectOption("GPT-4.1");
  const message = "Check the pending pull requests.";
  await page.locator("#message").fill(message);
  await page.getByRole("button", {name: "Send message", exact: true}).click();
  await expect(page.getByRole("alert")).toContainText("Couldn’t start your fleet");
  await expect(page.locator("#message")).toHaveValue(message);
  await page.getByRole("button", {name: "Resend", exact: true}).click();
  await expect(page.getByRole("region", {name: "Fleet conversation"}).getByText(message, {exact: true})).toBeVisible();
  await expect(model).toHaveValue("GPT-4.1");
});

test("clicking the title edits it in place and blur saves it", async ({page}) => {
  await page.goto(review("conversation"));
  const title = page.locator("#page-title");
  const input = page.getByRole("textbox", {name: "Chat name"});
  const titleBox = await title.boundingBox();
  await title.click();
  await expect(input).toBeFocused();
  await expect(input).toHaveCSS("background-color", "rgba(0, 0, 0, 0)");
  await expect(input).toHaveCSS("border-top-width", "0px");
  const editBox = await input.boundingBox();
  expect(editBox!.width).toBeLessThan(titleBox!.width + 8);
  expect(editBox!.height).toBeCloseTo(titleBox!.height, 0);
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await input.fill("Release checklist");
  await page.locator(".user-bubble").first().click();
  await expect(title).toHaveText("Release checklist");
  await expect(input).toBeHidden();
  await expect(title).toHaveAttribute("data-name-saved", "");

  await title.press("Enter");
  await input.fill("Discard this name");
  await input.press("Escape");
  await expect(title).toHaveText("Release checklist");
  await expect(title).toBeFocused();
});

test("model creation keeps the provider and model usable while saved keys load", async ({page}) => {
  await page.goto(review("model-loading"));
  const dialog = page.getByRole("dialog", {name: "Create model"});
  await expect(dialog.getByText("Checking saved keys…")).toBeVisible();
  await expect(dialog.getByRole("combobox", {name: "Provider"})).toBeEnabled();
  await expect(dialog.getByRole("combobox", {name: "Model", exact: true})).toBeEnabled();
  await expect(dialog.getByRole("button", {name: "Create model"})).toBeDisabled();
  await dialog.getByRole("radio", {name: "Add new key"}).check();
  await expect(dialog.getByText("Checking saved keys…")).toBeHidden();
  await expect(dialog.locator("#model-api-key")).toBeEditable();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("button", {name: "Add another model"})).toHaveCount(0);
});

test("Create model opens catalogue details within the model form", async ({page}) => {
  await page.goto(review("models"));
  await page.getByRole("button", {name: "Create model", exact: true}).click();
  const dialog = page.getByRole("dialog", {name: "Create model"});
  await dialog.getByRole("combobox", {name: "Provider"}).selectOption("anthropic");
  await dialog.getByRole("combobox", {name: "Model", exact: true}).selectOption("Fable 5.1");
  await dialog.getByRole("button", {name: "Model details"}).click();
  await expect(dialog.getByText("Price schedule (1)", {exact: true})).toBeVisible();
});

test("the conversation has one retry action and reports when the check succeeds", async ({page}) => {
  const time = new Date("2026-11-23T00:00:00Z");
  await page.clock.install({time});
  await page.clock.pauseAt(new Date(time.valueOf() + 1_000));
  await page.goto(review("failure"));
  await expect(page.locator(".error-card")).toHaveCount(0);
  await page.getByRole("button", {name: "Reconnect & retry"}).click();
  await expect(page.getByRole("button", {name: "Retrying…"})).toBeDisabled();
  await page.clock.fastForward(700);
  await expect(page.getByText("Reconnected. Release v2.8.1 is healthy.", {exact: true}).first()).toBeVisible();
  await expect(page.getByRole("button", {name: "Reconnect & retry"})).toHaveCount(0);
});

test("an addressed preview opens the new turn and quotes its streamed reply", async ({page}) => {
  const time = new Date("2026-11-23T00:00:00Z");
  await page.clock.install({time});
  await page.clock.pauseAt(new Date(time.valueOf() + 1_000));
  await page.emulateMedia({reducedMotion: "reduce"});
  await page.goto(review("bob"));
  const input = page.locator("#message");
  await input.fill("@release-watch Hello");
  await input.press("Enter");
  const reply = page.locator("[data-preview-reply]");
  await expect(page).toHaveURL(/#conversation$/);
  await expect(page.getByText("Hello", {exact: true})).toBeInViewport();
  await expect(reply.getByRole("status", {name: "Working"})).toBeVisible();
  await page.clock.runFor(2_000);
  const answer = "Hello! What would you like me to check on the release?";
  await expect(reply.getByText(answer, {exact: true})).toBeVisible();
  await reply.getByRole("button", {name: "Quote response"}).click();
  await expect(input).toHaveValue(`> ${answer}\n\n`);
});

test("stopping a waiting or streaming preview keeps the stopped result and composer focus", async ({page}) => {
  const time = new Date("2026-11-23T00:00:00Z");
  await page.clock.install({time});
  await page.clock.pauseAt(new Date(time.valueOf() + 1_000));
  await page.goto(review("conversation"));
  const input = page.locator("#message");
  for (const waiting of [true, false]) {
    await input.fill("Hello");
    await input.press("Enter");
    const reply = page.locator("[data-preview-reply]").last();
    if (waiting) await expect(reply.getByRole("status", {name: "Working"})).toBeVisible();
    else {
      await page.clock.runFor(550);
      await expect(reply.locator(".preview-reply-text")).toBeVisible();
    }
    await page.getByRole("button", {name: "Stop", exact: true}).click();
    await page.clock.runFor(2_000);
    await expect(reply.getByText("Stopped.", {exact: true})).toBeVisible();
    await expect(input).toBeFocused();
  }
});

test("the CLI tabs show sign-in progress and a delayed runner request", async ({page}) => {
  const time = new Date("2026-11-23T00:00:00Z");
  await page.clock.install({time});
  await page.clock.pauseAt(new Date(time.valueOf() + 1_000));
  await page.goto(review("cli-sign-in"));
  await page.getByRole("button", {name: "Authorize", exact: true}).click();
  await expect(page.locator("[data-action=authorize-cli]")).toBeDisabled();
  await expect(page.getByRole("status").filter({hasText: "Authorizing"})).toBeVisible();
  await page.clock.fastForward(900);
  await expect(page.getByRole("heading", {name: "Authorization granted"})).toBeVisible();
  await page.goto(review("terminal"));
  await page.getByRole("button", {name: "Replay a slow request"}).click();
  await page.clock.fastForward(400);
  await expect(page.getByRole("status").filter({hasText: "Waiting for agentsfleet"})).toBeVisible();
  await expect(page.locator(".orbit-ball")).toBeVisible();
  await page.clock.fastForward(1_050);
  await expect(page.locator("#runner-list-output")).toBeVisible();
  await expect(page.getByRole("status").filter({hasText: "Waiting for agentsfleet"})).toBeHidden();
  const color = (state: string) => page.locator(`#runner-list-output .${state}`).first().evaluate(node => getComputedStyle(node).color);
  expect(new Set(await Promise.all(["active", "online", "offline"].map(color))).size).toBe(3);
});

test("the reference shows requested scope and lets the user revoke approved access", async ({page}) => {
  await page.goto(review("conversation"));
  const request = page.getByRole("region", {name: "Needs your approval"});
  await expect(request.getByText("agentsfleet/platform-images", {exact: true})).toBeVisible();
  await expect(request.getByText("Read and write", {exact: true})).toBeVisible();
  await expect(request.getByRole("button", {name: "Deny"})).toBeVisible();
  await request.getByRole("button", {name: "Approve", exact: true}).click();
  const access = page.getByRole("region", {name: "Can access"});
  await access.getByRole("button", {name: "Revoke GitHub"}).click();
  await page.getByRole("dialog", {name: "Revoke this access?"}).getByRole("button", {name: "Revoke access"}).click();
  await expect(access).toContainText("No access approved yet");
});

test("the reference keeps completion editing in the fleet summary", async ({page}) => {
  await page.goto(review("conversation"));
  await page.locator("summary").filter({hasText: "Tasks"}).click();
  const edit = page.getByRole("button", {name: "Edit statements"});
  await edit.click();
  const form = page.getByRole("form", {name: "Edit completion statements"});
  await expect(form.getByRole("textbox", {name: "Statement 1"})).toBeFocused();
  await form.getByRole("textbox").fill("The image builds on Linux.");
  await form.getByRole("button", {name: "Save statements"}).click();
  await expect(form).toBeHidden();
  await expect(edit).toBeFocused();
  await expect(page.getByText("The image builds on Linux.", {exact: true})).toBeVisible();
});
