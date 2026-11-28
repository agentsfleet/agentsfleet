import {expect, test} from "@playwright/test";
import {signInAs} from "./fixtures/auth";
import {awaitTerminalTurn} from "./fixtures/chat-walk";
import {assertPassed, chatTurnIds, classifyTerminalTurn} from "./fixtures/execution";
import {workspaceHref} from "./fixtures/nav";
import {createPrimeFleet, entry, PRIME_FLEET, primeFleet, proofs} from "./fixtures/prime-fleets";
import {PRIME_PERSON} from "./fixtures/prime-walk";
import {getDefaultWorkspaceId} from "./fixtures/seed";
import {purgeFixtureFleet} from "./fixtures/teardown";

const fixture = primeFleet(PRIME_FLEET.keenHawk);
const proof = proofs(fixture, 1);

test(proof(0), async ({page}) => {
  test.setTimeout(420_000);
  const image = fixture.image_png;
  if (!image) throw new Error("Keen Hawk has no image fixture");
  const workspace = await getDefaultWorkspaceId(PRIME_PERSON);
  const fleet = await createPrimeFleet(PRIME_PERSON, workspace, fixture);
  try {
    await signInAs(page, PRIME_PERSON);
    const seen = await chatTurnIds(PRIME_PERSON, workspace, fleet.id);
    await page.goto(workspaceHref(workspace, `fleets/${fleet.id}`));
    const composer = page.getByRole("form", {name: "Chat composer"});
    const chooser = page.waitForEvent("filechooser");
    await composer.getByRole("button", {name: "Attach a preview file"}).click();
    await (await chooser).setFiles({name: "picture.png", mimeType: "image/png", buffer: Buffer.from(image, "base64")});
    const question = entry(fixture, fixture.follow_ups, 0, "image question");
    await composer.getByRole("textbox").fill(question);
    await composer.getByRole("button", {name: "Send", exact: true}).click();

    const turn = await awaitTerminalTurn(workspace, fleet.id, seen);
    assertPassed(classifyTerminalTurn(turn));
    expect(JSON.parse(turn.request_json)).toMatchObject({
      message: question, images: [{media_type: "image/png", data: image}],
    });
    expect(turn.response_text).toMatch(/\bred\b/i);
    await page.reload();
    const chat = page.getByRole("log", {name: "Chat"});
    await expect(chat.getByText(question, {exact: true})).toBeVisible();
    await expect(chat.locator('[data-role="assistant"]').last()).toContainText(/\bred\b/i);
  } finally {
    await purgeFixtureFleet(PRIME_PERSON, workspace, fleet.id);
  }
});
