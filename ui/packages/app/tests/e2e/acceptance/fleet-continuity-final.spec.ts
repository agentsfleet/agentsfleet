import { expect, test } from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { ContinuityJourney } from "./fixtures/continuity-journey";
import { CONTINUITY } from "./fixtures/continuity-scenario";
import { ensureSecondWorkspace } from "./fixtures/seed";
import { uniqueTag } from "./fixtures/observation";

const WORKSPACE_NAME = "fixture-final-continuity";

test.describe("final real fleet continuity acceptance", () => {
  test("one fleet retains 181 messages through four saved setups and home navigation", async ({page}) => {
    test.setTimeout(CONTINUITY.singleTimeoutMs);
    const {id: workspace} = await ensureSecondWorkspace(FIXTURE_KEY.regular, WORKSPACE_NAME);
    const tag = uniqueTag();
    await signInAs(page, FIXTURE_KEY.regular);
    const journey = new ContinuityJourney(page, FIXTURE_KEY.regular, workspace, `continuity-single-${tag}`, `single-${tag}`);
    try {
      await journey.run();
    } finally {
      try {
        await test.info().attach("single-history", {body: JSON.stringify(await journey.snapshot()), contentType: "application/json"});
        await page.screenshot({path: test.info().outputPath("single-final.png"), fullPage: true});
      } finally {
        await journey.cleanup();
      }
    }
  });

  test("ten concurrent fleets preserve 1810 messages and separate setup versions", async ({context}) => {
    test.setTimeout(CONTINUITY.concurrentTimeoutMs);
    const {id: workspace} = await ensureSecondWorkspace(FIXTURE_KEY.regular, WORKSPACE_NAME);
    const tag = uniqueTag();
    const entry = await context.newPage();
    await signInAs(entry, FIXTURE_KEY.regular);
    // Ten pages share one signed-in person, each with its own fleet, draft and stream.
    const pages = await Promise.all(Array.from({length: CONTINUITY.concurrentFleets}, () => context.newPage()));
    await entry.close();
    const journeys = pages.map((page, index) => new ContinuityJourney(page, FIXTURE_KEY.regular, workspace,
      `continuity-concurrent-${tag}-${index}`, `concurrent-${tag}-${index}`));
    try {
      // Fixed, explicit concurrency. All outcomes are inspected before any cleanup.
      const results = await Promise.allSettled(journeys.map(journey => journey.run()));
      const failures = results.flatMap((result, index) => result.status === "rejected"
        ? [{fleet: journeys[index]!.name, error: String(result.reason)}] : []);
      expect(failures, "every concurrent fleet completed its entire lifecycle").toEqual([]);
      await Promise.all(journeys.map(journey => journey.assertIsolated(
        journeys.filter(other => other !== journey).map(other => other.marker))));
    } finally {
      const cleanup = await Promise.allSettled(journeys.map(async (journey, index) => {
        try {
          await test.info().attach(`concurrent-${index}-history`, {body: JSON.stringify(await journey.snapshot()), contentType: "application/json"});
          await pages[index]!.screenshot({path: test.info().outputPath(`concurrent-${index}.png`)});
        } finally {
          await journey.cleanup();
          await pages[index]!.close();
        }
      }));
      expect(cleanup.filter(result => result.status === "rejected"), "all owned continuity fixtures cleaned up").toEqual([]);
    }
  });
});
