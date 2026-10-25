import {expect, it, vi} from "vitest";
import {withContinuityEvidence} from "./e2e/acceptance/fixtures/continuity-evidence";

const INTERRUPTED = new Error("provider request interrupted");
const UNAVAILABLE = new Error("history unavailable");

it("records every failed journey and never deletes its live evidence", async () => {
  const journeys = [{cleanup: vi.fn()}, {cleanup: vi.fn()}];
  const record = vi.fn().mockResolvedValue(undefined);
  const operation = withContinuityEvidence(journeys, async () => {throw INTERRUPTED;}, record);
  await expect(operation).rejects.toMatchObject({errors: [INTERRUPTED]});
  expect(record).toHaveBeenCalledTimes(journeys.length);
  for (const journey of journeys) expect(journey.cleanup).not.toHaveBeenCalled();
});

it("captures identifiers after a synchronous execution failure", async () => {
  const journey = {cleanup: vi.fn()};
  const record = vi.fn().mockResolvedValue(undefined);
  const operation = withContinuityEvidence([journey], () => {throw INTERRUPTED;}, record);
  await expect(operation).rejects.toMatchObject({errors: [INTERRUPTED]});
  expect(record).toHaveBeenCalledWith(journey, 0);
  expect(journey.cleanup).not.toHaveBeenCalled();
});

it("preserves the whole batch if one evidence read throws and still records the others", async () => {
  const journeys = [{cleanup: vi.fn()}, {cleanup: vi.fn()}];
  const record = vi.fn((_journey, index: number) => {if (index === 0) throw UNAVAILABLE; return Promise.resolve();});
  const operation = withContinuityEvidence(journeys, async () => {}, record);
  await expect(operation).rejects.toMatchObject({errors: [UNAVAILABLE]});
  expect(record).toHaveBeenCalledTimes(journeys.length);
  for (const journey of journeys) expect(journey.cleanup).not.toHaveBeenCalled();
});

it("retains both the original failure and an evidence failure", async () => {
  const journey = {cleanup: vi.fn()};
  const operation = withContinuityEvidence([journey], async () => {throw INTERRUPTED;}, async () => {throw UNAVAILABLE;});
  await expect(operation).rejects.toMatchObject({errors: [INTERRUPTED, UNAVAILABLE]});
  expect(journey.cleanup).not.toHaveBeenCalled();
});

it("cleans successful journeys only after all their evidence has been captured", async () => {
  const observed = new Set<number>();
  const journeys = Array.from({length: 2}, () => ({cleanup: vi.fn(async () => {expect(observed.size).toBe(2);})}));
  await withContinuityEvidence(journeys, async () => {}, async (_journey, index) => {observed.add(index);});
  for (const journey of journeys) expect(journey.cleanup).toHaveBeenCalledOnce();
});

it("reports cleanup failures and attempts every owned cleanup", async () => {
  const journeys = [{cleanup: vi.fn(() => {throw UNAVAILABLE;})}, {cleanup: vi.fn(async () => {})}];
  const operation = withContinuityEvidence(journeys, async () => {}, async () => {});
  await expect(operation).rejects.toMatchObject({errors: [UNAVAILABLE]});
  for (const journey of journeys) expect(journey.cleanup).toHaveBeenCalledOnce();
});
