import {expect, test, type Page} from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { ContinuityJourney } from "./fixtures/continuity-journey";
import { withContinuityEvidence } from "./fixtures/continuity-evidence";
import {CONTINUITY, CONTINUITY_FLEET_PREFIX} from "./fixtures/continuity-scenario";
import { ensureSecondWorkspace } from "./fixtures/seed";
import { uniqueTag } from "./fixtures/observation";

const WORKSPACE_NAME = "fixture-final-continuity";
const JSON_CONTENT_TYPE = "application/json";

async function recordJourney(journey: ContinuityJourney, label: string, page: Page): Promise<void> {
  await test.info().attach(`${label}-identifiers`, {body: JSON.stringify(journey.identifiers()), contentType: JSON_CONTENT_TYPE});
  await test.info().attach(`${label}-history`, {body: JSON.stringify(await journey.snapshot()), contentType: JSON_CONTENT_TYPE});
  await page.screenshot({path: test.info().outputPath(`${label}-final.png`), fullPage: true});
}

test.describe("final real fleet continuity acceptance", () => {
  test("one fleet retains 181 messages through four saved setups and home navigation", async ({page}) => {
    test.setTimeout(CONTINUITY.singleTimeoutMs);
    const {id: workspace} = await ensureSecondWorkspace(FIXTURE_KEY.regular, WORKSPACE_NAME);
    const tag = uniqueTag();
    await signInAs(page, FIXTURE_KEY.regular);
    const journey = new ContinuityJourney(page, FIXTURE_KEY.regular, workspace, `${CONTINUITY_FLEET_PREFIX}single-${tag}`, `single-${tag}`);
    await withContinuityEvidence([journey], () => journey.run(), async () => recordJourney(journey, "single", page));
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
      `${CONTINUITY_FLEET_PREFIX}concurrent-${tag}-${index}`, `concurrent-${tag}-${index}`));
    await withContinuityEvidence(journeys, async () => {
      // Fixed, explicit concurrency. All outcomes are inspected before any cleanup.
      const results = await Promise.allSettled(journeys.map(journey => journey.run()));
      const failures = results.flatMap((result, index) => result.status === "rejected"
        ? [{fleet: journeys[index]!.name, error: String(result.reason)}] : []);
      expect(failures, "every concurrent fleet completed its entire lifecycle").toEqual([]);
      await Promise.all(journeys.map(journey => journey.assertIsolated(
        journeys.filter(other => other !== journey).map(other => other.marker))));
    }, async (journey, index) => recordJourney(journey, `concurrent-${index}`, pages[index]!));
  });
});
