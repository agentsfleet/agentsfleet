import {expect, test, type BrowserContext, type Page} from "@playwright/test";
import { signInAs } from "./fixtures/auth";
import { FIXTURE_KEY } from "./fixtures/constants";
import { SoakJourney, type SoakManifest } from "./fixtures/soak-journey";
import { outageLedger, outageWatch, pendingRequests } from "./fixtures/soak-outage";
import { readFileSync } from "node:fs";
import * as path from "node:path";
import { withSoakEvidence } from "./fixtures/soak-evidence";
import {SOAK, SOAK_FLEET_PREFIX} from "./fixtures/soak-scenario";
import { ensureSecondWorkspace } from "./fixtures/seed";
import { uniqueTag } from "./fixtures/observation";

const WORKSPACE_NAME = "fixture-soak";
const JSON_CONTENT_TYPE = "application/json";
// An unrelated host built for connectivity checks: if it fails too, the outage is not api-dev's.
const NEUTRAL_PROBE = "https://www.gstatic.com/generate_204";

/** Resume manifests from a previous process (SOAK_RESUME), in journey order. */
function resumeManifests(): SoakManifest[] {
  const file = process.env.SOAK_RESUME;
  if (!file) return [];
  const parsed: unknown = JSON.parse(readFileSync(file, "utf8"));
  return (Array.isArray(parsed) ? parsed : [parsed]) as SoakManifest[];
}

function checkpointPath(marker: string): string | undefined {
  const dir = process.env.SOAK_CHECKPOINT_DIR;
  return dir ? path.join(dir, `checkpoint-${marker}.json`) : undefined;
}

test.beforeAll(() => outageWatch.start(process.env.NEXT_PUBLIC_API_URL!, NEUTRAL_PROBE));
test.afterAll(async () => {
  outageWatch.stop();
  console.log(JSON.stringify({soakOutages: outageLedger}));
});

async function recordJourney(journey: SoakJourney, label: string, page: Page): Promise<void> {
  await test.info().attach(`${label}-identifiers`, {body: JSON.stringify(journey.identifiers()), contentType: JSON_CONTENT_TYPE});
  await test.info().attach(`${label}-outages`, {body: JSON.stringify(outageLedger.filter(entry => entry.fleet === journey.name)), contentType: JSON_CONTENT_TYPE});
  await test.info().attach(`${label}-history`, {body: JSON.stringify(await journey.snapshot()), contentType: JSON_CONTENT_TYPE});
  await test.info().attach(`${label}-pending-requests`, {body: JSON.stringify(pendingRequests(page)), contentType: JSON_CONTENT_TYPE});
  await page.screenshot({path: test.info().outputPath(`${label}-final.png`), fullPage: true});
}

test.describe("final real fleet soak acceptance", () => {
  test.skip(process.env.AGENTSFLEET_ACCEPTANCE_SCOPE === "post-deploy", "Long soak runs locally or in the separate api-dev job");
  test("one fleet retains 181 messages through four saved setups and home navigation", async ({page}) => {
    test.setTimeout(SOAK.singleTimeoutMs);
    const {id: workspace} = await ensureSecondWorkspace(FIXTURE_KEY.regular, WORKSPACE_NAME);
    const [resume] = resumeManifests();
    const tag = uniqueTag();
    await signInAs(page, FIXTURE_KEY.regular);
    const name = resume?.name ?? `${SOAK_FLEET_PREFIX}single-${tag}`;
    const marker = resume?.marker ?? `single-${tag}`;
    const journey = new SoakJourney(page, FIXTURE_KEY.regular, resume?.workspace ?? workspace, name, marker, checkpointPath(marker));
    await withSoakEvidence([journey], () => journey.run(resume), async () => recordJourney(journey, "single", page));
  });

  test("soak mini: two concurrent fleets keep separate histories and setup versions", {tag: "@soak-mini"}, async ({context}) => {
    await concurrentSoak(context, SOAK.miniFleets, "mini");
  });

  test("soak marathon: ten concurrent fleets keep separate histories and setup versions", {tag: ["@soak-marathon", "@ten-fleets"]}, async ({context}) => {
    await concurrentSoak(context, SOAK.marathonFleets, "marathon");
  });
});

/** Runs `count` full journeys at once, one page each, and inspects every outcome. */
async function concurrentSoak(context: BrowserContext, count: number, kind: string): Promise<void> {
  test.setTimeout(SOAK.concurrentTimeoutMs);
  const {id: workspace} = await ensureSecondWorkspace(FIXTURE_KEY.regular, WORKSPACE_NAME);
  const tag = uniqueTag();
  const entry = await context.newPage();
  await signInAs(entry, FIXTURE_KEY.regular);
  // Every page shares one signed-in person, each with its own fleet, draft and stream.
  const pages = await Promise.all(Array.from({length: count}, () => context.newPage()));
  await entry.close();
  const resumes = resumeManifests();
  expect([0, count], "a resumed run names every fleet").toContain(resumes.length);
  const journeys = pages.map((page, index) => {
    const name = resumes[index]?.name ?? `${SOAK_FLEET_PREFIX}${kind}-${tag}-${index}`;
    const marker = resumes[index]?.marker ?? `${kind}-${tag}-${index}`;
    return new SoakJourney(page, FIXTURE_KEY.regular, resumes[index]?.workspace ?? workspace, name, marker, checkpointPath(marker));
  });
  await withSoakEvidence(journeys, async () => {
    const results = await Promise.allSettled(journeys.map((journey, index) => journey.run(resumes[index])));
    const failures = results.flatMap((result, index) => result.status === "rejected"
      ? [{fleet: journeys[index]!.name, error: String(result.reason)}] : []);
    expect(failures, "every concurrent fleet completed its entire lifecycle").toEqual([]);
    await Promise.all(journeys.map(journey => journey.assertIsolated(
      journeys.filter(other => other !== journey).map(other => other.marker))));
  }, async (journey, index) => recordJourney(journey, `${kind}-${index}`, pages[index]!));
}
