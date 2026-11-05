import { test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";

// Local-only: leaves a signed-in, headed browser on SOAK_HOLD_URL until a person closes it.
test("hold an authenticated browser on a retained fixture", async ({page}) => {
  const url = process.env.SOAK_HOLD_URL;
  test.skip(!url, "SOAK_HOLD_URL is unset");
  test.setTimeout(0);
  await signInAs(page, FIXTURE_KEY.regular);
  await page.goto(url!);
  console.log(JSON.stringify({held: page.url()}));
  await page.waitForEvent("close", {timeout: 0});
});
