import {expect, type Locator, type Page, type TestInfo} from "@playwright/test";
import {test} from "./fixtures/chat-probe";

const text = ["font-family", "font-size", "line-height", "font-weight", "letter-spacing", "color"];
const panel = ["width", "border-radius", "border-color", "background-color", "padding-left", "padding-top"];
const icon = ["width", "height", "color"];
const field = [...text, "width", "height", "border-radius", "background-color"];

test("browsing models and choosing a fleet model keep provider keys out of the page", async ({page, chatProbe}) => {
  const passwords = page.locator('input[type="password"]');
  await page.goto(`${chatProbe}?research=1`);
  await page.getByRole("combobox", {name: "Fleet model"}).click();
  await expect(page.getByRole("listbox")).toBeVisible();
  await expect(passwords).toHaveCount(0);

  await page.goto(`${chatProbe}?resources=1&parity=1`);
  const create = page.getByRole("button", {name: "Create model", exact: true});
  await expect(create).toBeVisible();
  await create.focus();
  await expect(passwords).toHaveCount(0);
  await create.click();
  const key = page.getByRole("dialog").getByLabel("API key", {exact: true});
  await expect(key).toHaveAttribute("type", "password");
  await expect(key).toHaveAttribute("data-1p-ignore", "");
  await key.fill("test-provider-key");
  await expect(key).toHaveValue("test-provider-key");
  await page.keyboard.press("Escape");
  await expect(passwords).toHaveCount(0);

  await page.getByRole("button", {name: "Manage gpt-4.1"}).click();
  await page.getByRole("menuitem", {name: "Edit model"}).click();
  await expect(key).toHaveAttribute("data-1p-ignore", "");
  await expect(key).toHaveValue("");
  await key.fill("test-replacement-key");
  await expect(key).toHaveValue("test-replacement-key");
  await page.keyboard.press("Escape");
  await expect(passwords).toHaveCount(0);
});

for (const view of ["Models", "Team"] as const) {
  test(`${view} matches the reference at both palettes and supports its keyboard workflows`, async ({page, chatProbe, isMobile}, info) => {
    const reference = await page.context().newPage();
    const viewport = isMobile ? {width: 390, height: 844} : {width: 1440, height: 1000};
    for (const surface of [page, reference]) {
      await surface.setViewportSize(viewport);
      await surface.emulateMedia({reducedMotion: "reduce"});
    }
    await Promise.all([
      page.goto(`${chatProbe}?resources=1&parity=1&visual=1`),
      reference.goto(new URL(`../../../../../docs/ux.html#${view.toLowerCase()}`, import.meta.url).href),
    ]);
    if (view === "Team") await page.getByRole("button", {name: "Team", exact: true}).click();
    const title = page.getByRole("heading", {level: 1, name: view});
    const designTitle = reference.locator(".page-heading h2");
    const trigger = page.getByRole("button", {name: view === "Models" ? "Create model" : "Invite member", exact: true});
    const designTrigger = reference.locator(".page-heading .primary");

    for (const palette of ["dark", "light"]) {
      for (const surface of [page, reference]) {
        await surface.evaluate(async theme => {
          document.documentElement.dataset.theme = theme;
          await document.fonts.ready;
        }, palette);
      }
      await match(title, designTitle, text);
      await match(title.locator("..").locator("p"), reference.locator(".page-heading p"), text);
      await match(trigger, designTrigger, [...text, "width", "height", "gap", "border-radius", "background-color"]);
      await matchGlyph(trigger.locator("svg"), designTrigger.locator("i"));
      if (view === "Models") await reviewModels(page, reference);
      else await reviewTeam(page, reference, isMobile);
      await capture(page, reference, info, view.toLowerCase(), palette);
      if (view === "Team") {
        const manage = page.getByRole("button", {name: "Manage invite for sofia.chen@example.com"});
        await manage.press("Enter");
        await reference.getByRole("button", {name: "Manage invite for sofia.chen@example.com"}).click();
        const menu = page.getByRole("menu"), refDialog = reference.getByRole("dialog");
        const actions = [
          ["Send invite again", "resend"], ["Copy invite link for sofia.chen@example.com", "copy"], ["Revoke invite", "revoke"],
        ] as const;
        for (const [name, action] of actions) {
          const item = menu.getByRole("menuitem", {name});
          const designItem = refDialog.locator(`[data-team-action=${action}]`);
          await matchGlyph(item.locator("svg"), designItem.locator("i"));
        }
        await capture(page, reference, info, "team-invitation-actions", palette);
        await page.keyboard.press("Escape");
        await reference.keyboard.press("Escape");
        await expect(manage).toBeFocused();
        for (const heading of [title, designTitle]) await heading.evaluate(element => {
          for (let parent = element.parentElement; parent; parent = parent.parentElement) parent.scrollTop = 0;
        });
      }

      if (isMobile) {
        const appAnchor = view === "Models" ? page.getByRole("region", {name: "Your models"}) : page.getByRole("listitem", {name: "sofia.chen@example.com"});
        const refAnchor = reference.locator(view === "Models" ? ".page-note" : ".invite-row:last-child");
        for (const anchor of [appAnchor, refAnchor]) await anchor.evaluate(element => element.scrollIntoView({block: "end"}));
        await capture(page, reference, info, `${view.toLowerCase()}-lower`, palette);
        for (const anchor of [title, designTitle]) await anchor.evaluate(element => {
          for (let parent = element.parentElement; parent; parent = parent.parentElement) parent.scrollTop = 0;
        });
      }
      await trigger.press("Enter");
      await designTrigger.click();
      const dialog = page.getByRole("dialog"), design = reference.getByRole("dialog");
      await match(dialog, design, panel);
      await match(dialog.getByRole("heading"), design.locator(".modal-heading h2"), ["font-family", "font-size", "font-weight", "color"]);
      await matchGlyph(dialog.locator("[data-dialog-close] svg"), design.locator(".modal-heading button i"));
      await match(dialog.getByRole("button", {name: "Cancel"}), design.locator(".modal-actions .secondary"), [...text, "height", "width", "border-radius"]);
      const refInput = design.locator(view === "Models" ? "#new-model-provider" : "#team-invite-email");
      const input = view === "Models" ? dialog.getByRole("combobox", {name: "Provider", exact: true}) : dialog.getByRole("textbox", {name: "Email address"});
      if (view === "Models") {
        // The reference opens with OpenAI selected. Production asks the owner to choose.
        await input.press("Enter");
        await page.getByRole("option", {name: "OpenAI", exact: true}).press("Enter");
      }
      await match(input, refInput, field);
      expect(await topOffset(dialog, input)).toBeCloseTo(await topOffset(design, refInput), 0);
      if (view === "Models") {
        const caret = input.locator("[data-slot=select-caret]");
        await expect(caret).toHaveCSS("opacity", "1");
        await match(caret.locator("svg"), design.locator(".model-description"), ["color"]);
        await expect(caret.locator("svg")).toHaveCSS("width", "16px");
        await expect(refInput).toHaveCSS("background-size", "16px 16px");
        await expect(refInput).toHaveCSS("background-position-x", /12px/);
        const inset = await input.evaluate(node => {
          const arrow = node.querySelector("[data-slot=select-caret] svg")!;
          return node.getBoundingClientRect().right - arrow.getBoundingClientRect().right - parseFloat(getComputedStyle(node).borderRightWidth);
        });
        expect(inset).toBe(12);
        await expect(dialog.getByLabel("Key name", {exact: true})).toBeVisible();
        await match(dialog.getByRole("radiogroup"), design.locator(".credential-tabs"), ["width", "height", "background-color", "border-color", "border-radius", "gap"]);
        await match(dialog.getByLabel("Key name", {exact: true}), design.locator("#model-key-name"), field);
        await match(dialog.getByText("Provider API key", {exact: true}), design.locator(".credential-options legend"));
        await match(dialog.getByText("Add new key", {exact: true}), design.locator(".credential-tabs label").first());
        await match(dialog.getByText("Make active", {exact: true}), design.locator(".model-default-choice strong"));
        await match(dialog.getByText("Use this as your default model for new work."), design.locator(".model-default-choice small"));
      } else {
        await expect(input).toBeFocused();
        await expect(input).toHaveAccessibleDescription(/every workspace in this account/);
        await expect(dialog.getByText("Kishore’s account", {exact: true})).toBeVisible();
        const scope = dialog.locator("p").filter({hasText: "They’ll join as a"});
        await match(scope, design.locator(".team-invite-scope"), panel);
        await matchGlyph(scope.locator("svg"), design.locator(".team-invite-scope i"));
        expect(await topOffset(input, scope)).toBe(await topOffset(refInput, design.locator(".team-invite-scope")));
      }
      await capture(page, reference, info, `${view.toLowerCase()}-dialog`, palette);
      await dialog.getByRole("button", {name: "Close", exact: true}).focus();
      await page.keyboard.press("Tab");
      expect(await dialog.evaluate(element => element.contains(document.activeElement))).toBe(true);
      await page.keyboard.press("Escape");
      await expect(trigger).toBeFocused();
      await reference.keyboard.press("Escape");
      await expect(design).toBeHidden();
    }

    if (view === "Team") await useTeam(page);
    else await useModels(page);
    await reference.close();
  });
}

async function reviewModels(page: Page, reference: Page) {
  const card = page.getByLabel("Platform default", {exact: true});
  const design = reference.locator(".models-default");
  await match(card, design, panel);
  await matchGlyph(card.locator("svg"), design.locator(".resource-icon i"));
  await match(card.locator("h2"), design.locator("strong"), text);
  await match(card.locator("p"), design.locator("p"), text);
  await match(card.getByText("Active", {exact: true}), design.locator(".model-active"), [...text, "gap"]);
  const section = page.getByRole("region", {name: "Your models"});
  const heading = section.getByRole("heading");
  const designHeading = reference.locator(".section-title h3");
  await match(heading, designHeading, [...text, "height"]);
  await matchCountGeometry(heading, designHeading);
  await match(section.getByRole("listitem"), reference.locator(".model-row"), ["height", "width", "padding-left", "padding-top", "gap"]);
  await match(section.getByText("OpenAI", {exact: true}), reference.locator(".resource-copy p"), text);
  await matchGlyph(section.locator("p svg"), reference.locator(".model-row small i"));
  await matchGlyph(section.getByRole("listitem").locator(":scope > span svg"), reference.locator(".model-row .resource-icon i"));
  await expect(page.getByRole("button", {name: "Add another model"})).toHaveCount(0);
  await match(section.getByText("GPT-4.1", {exact: true}), reference.locator(".resource-copy strong"));
  await match(section.getByText("OpenAI workspace", {exact: false}), reference.locator(".model-row small"));
}

async function reviewTeam(page: Page, reference: Page, isMobile: boolean) {
  const scope = page.getByText("Kishore’s account", {exact: true}).locator("../..");
  await match(scope, reference.locator(".team-scope"), ["width", "height", "padding-bottom", "gap", "border-bottom-color"]);
  await matchGlyph(scope.locator("svg"), reference.locator(".team-scope-icon i"));
  const search = page.getByRole("searchbox");
  await match(search, reference.locator("#team-search"), text);
  await matchGlyph(search.locator("..").locator("svg"), reference.locator(".search i"));
  await matchCountGeometry(page.getByRole("region", {name: "Members", exact: true}).getByRole("heading"), reference.locator("#members-title"));
  await matchCountGeometry(page.getByRole("region", {name: "Pending invitations", exact: true}).getByRole("heading"), reference.locator("#invitations-title"));
  const row = page.getByRole("listitem", {name: "Kishore", exact: true});
  await match(row, reference.locator(".team-row").first(), ["height", "width", "padding-left", "padding-top"]);
  await match(row.getByText("kishore@example.com"), reference.locator(".team-person > span").first(), text);
  await match(page.getByText("Email not sent · Share the invite link"), reference.locator(".team-warning"), text);
  await matchGlyph(page.getByRole("button", {name: "Manage Maya Rao"}).locator("svg"), reference.locator(".team-row [data-team-action=member] i"));
  await matchGlyph(page.getByRole("button", {name: "Manage invite for leon.park@example.com"}).locator("svg"), reference.locator(".invite-row button i").first());
  await matchGlyph(page.getByRole("listitem", {name: "leon.park@example.com"}).locator(":scope > span svg"), reference.locator(".invite-avatar i").first());
  await match(scope.getByText("Kishore’s account", {exact: true}), reference.locator(".team-scope strong"));
  await match(row.getByText("You", {exact: true}), reference.locator(".team-you"));
  await match(row.getByText("owner", {exact: true}), reference.locator(".team-role").first());
  await match(row.locator("time"), reference.locator(".team-date").first());
  await match(page.getByText("Account-wide access", {exact: true}), reference.locator(".team-toolbar .subtle"));
  const avatar = row.locator(":scope > [aria-hidden]").first();
  expect(await avatar.isVisible()).toBe(!isMobile);
  if (!isMobile) await match(avatar, reference.locator(".team-avatar").first(), [...text, "width", "height", "background-color"]);
}

async function matchCountGeometry(heading: Locator, designHeading: Locator) {
  await match(heading.locator("span"), designHeading.locator(".section-count"), [...text, "border-radius"]);
  const measure = (locator: Locator) => locator.evaluate(element => {
    const badge = element.lastElementChild;
    const title = element.firstChild;
    if (!badge || !title) throw new Error("The section heading is missing its label or count");
    const label = document.createRange();
    label.selectNode(title);
    const originalCount = badge.textContent;
    const geometry = ["0", "1", "2", "10"].map(count => {
      badge.textContent = count;
      const digits = document.createRange();
      digits.selectNodeContents(badge);
      const box = badge.getBoundingClientRect();
      const labelBox = label.getBoundingClientRect();
      const digitBox = digits.getBoundingClientRect();
      return {
        width: box.width,
        height: box.height,
        gap: box.left - labelBox.right,
        centerOffset: box.top + box.height / 2 - labelBox.top - labelBox.height / 2,
        digitPadding: [digitBox.left - box.left, box.right - digitBox.right, digitBox.top - box.top, box.bottom - digitBox.bottom],
      };
    });
    badge.textContent = originalCount;
    return geometry;
  });
  const [actual, reference] = await Promise.all([measure(heading), measure(designHeading)]);
  expect(actual).toEqual(reference);
  for (const count of actual.slice(0, 3)) {
    expect(count).toMatchObject({width: 25, height: 26, gap: 8, centerOffset: 0});
  }
}

async function useTeam(page: Page) {
  const search = page.getByRole("searchbox");
  await search.fill("maya");
  await expect(page.getByRole("heading", {name: "Members 2"})).toBeVisible();
  await expect(page.getByText("No invitations match your search.")).toBeVisible();
  const manage = page.getByRole("button", {name: "Manage Maya Rao"});
  await manage.press("Enter");
  await expect(page.getByRole("menuitem", {name: "Remove Maya Rao"})).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(manage).toBeFocused();
  await manage.press("Enter");
  await page.getByRole("menuitem", {name: "Remove Maya Rao"}).press("Enter");
  const confirm = page.getByRole("alertdialog", {name: "Remove Maya Rao?"});
  await expect(confirm.getByRole("heading")).toHaveCSS("font-family", /Manrope/);
  await expect(confirm.getByRole("heading")).toHaveCSS("font-size", "18px");
  await page.keyboard.press("Escape");
  await search.fill("absent");
  await expect(page.getByText("No members match your search.")).toBeVisible();
  await page.getByRole("button", {name: "Clear search"}).press("Enter");
  await expect(page.getByRole("listitem", {name: "leon.park@example.com"})).toBeVisible();
}

async function useModels(page: Page) {
  const manage = page.getByRole("button", {name: "Manage gpt-4.1"});
  await manage.press("Enter");
  await expect(page.getByRole("menuitem", {name: "Model details"})).toBeFocused();
  await page.getByRole("menuitem", {name: "Model details"}).press("Enter");
  const details = page.getByRole("dialog", {name: "GPT-4.1"});
  await expect(details.getByRole("heading", {name: "GPT-4.1"})).toHaveCSS("font-size", "18px");
  await expect(details.getByText("Key stored", {exact: true}).locator("svg")).toHaveCSS("width", "12px");
  await expect(details.locator("dd").filter({hasText: "gpt-4.1"}).first()).toHaveCSS("font-family", /mono/i);
  await page.keyboard.press("Escape");
  await manage.press("Enter");
  await page.getByRole("menuitem", {name: "Edit model"}).press("Enter");
  await expect(page.getByRole("dialog").getByRole("heading", {name: "Edit “gpt-4.1”"})).toHaveCSS("font-weight", "650");
  await page.keyboard.press("Escape");
  await manage.press("Enter");
  await page.getByRole("menuitem", {name: "Remove model"}).press("Enter");
  await expect(page.getByRole("alertdialog").getByRole("heading")).toHaveCSS("font-size", "18px");
  await page.keyboard.press("Escape");
  await page.getByRole("button", {name: "Create model", exact: true}).press("Enter");
  const dialog = page.getByRole("dialog");
  await dialog.getByRole("combobox", {name: "Provider"}).press("Enter");
  await page.getByRole("option", {name: "OpenAI", exact: true}).press("Enter");
  await dialog.getByRole("radio", {name: "Add new key"}).focus();
  await page.keyboard.down("ArrowRight");
  try { await expect(dialog.getByRole("radio", {name: "Use saved key"})).toBeChecked(); }
  finally { await page.keyboard.up("ArrowRight"); }
  await expect(dialog.getByRole("combobox", {name: "Saved key"})).toBeEnabled();
  await expect(dialog.getByLabel("API key", {exact: true})).toHaveCount(0);
}

async function matchGlyph(actual: Locator, reference: Locator) {
  await match(actual, reference, icon);
  const signature = (svg: Locator) => svg.evaluate(node => ({
    viewBox: node.getAttribute("viewBox"),
    paths: Array.from(node.querySelectorAll("path"), path => path.getAttribute("d")),
  }));
  const designSvg = reference.locator("svg");
  expect(await signature(actual), reference.toString()).toEqual(await signature(designSvg));
  await match(actual, designSvg, ["fill"]);
  const offset = (glyph: Locator) => glyph.evaluate(node => {
    const control = node.closest("button, [role=menuitem]");
    if (!control) return null;
    const a = node.getBoundingClientRect(), b = control.getBoundingClientRect();
    return a.top + a.height / 2 - b.top - b.height / 2;
  });
  const expected = await offset(designSvg);
  if (expected !== null) expect(Math.abs((await offset(actual))! - expected)).toBeLessThanOrEqual(1);
}

async function match(actual: Locator, reference: Locator, properties = text) {
  const styles = (locator: Locator) => locator.evaluate((element, names) => {
    const css = getComputedStyle(element);
    return Object.fromEntries(names.map(name => {
      if (name === "width" || name === "height") return [name, Math.round(element.getBoundingClientRect()[name])];
      const value = css.getPropertyValue(name);
      if (name === "letter-spacing" && value === "normal") return [name, "0px"];
      return [name, name === "font-family" ? value.split(",")[0]?.replaceAll('"', "").replace(" Variable", "") : value];
    }));
  }, properties);
  await expect.poll(() => styles(actual), {message: actual.toString()}).toEqual(await styles(reference));
}

async function topOffset(from: Locator, to: Locator) {
  const a = (await from.boundingBox())!, b = (await to.boundingBox())!;
  return Math.round(b.y - a.y);
}

async function capture(page: Page, reference: Page, info: TestInfo, state: string, theme: string) {
  for (const [label, surface] of [["app", page], ["reference", reference]] as const) {
    expect(await surface.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    const path = info.outputPath(`${label}-${state}-${theme}.png`);
    await surface.screenshot({path, animations: "disabled", scale: "css"});
  }
}
