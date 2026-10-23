type OwnedJourney = {cleanup: () => Promise<void>};

/** A failed run or failed evidence capture leaves every related fixture live. */
export async function withContinuityEvidence<T extends OwnedJourney>(
  journeys: readonly T[], run: () => Promise<void>, record: (journey: T, index: number) => Promise<void>,
): Promise<void> {
  const [execution] = await Promise.allSettled([Promise.resolve().then(run)]);
  const evidence = await Promise.allSettled(journeys.map(async (journey, index) => record(journey, index)));
  const failures = [execution, ...evidence].flatMap(result => result.status === "rejected" ? [result.reason] : []);
  if (failures.length) throw new AggregateError(failures, "Continuity failed; live fixture records were preserved");
  const cleanup = await Promise.allSettled(journeys.map(async journey => journey.cleanup()));
  const cleanupFailures = cleanup.flatMap(result => result.status === "rejected" ? [result.reason] : []);
  if (cleanupFailures.length) throw new AggregateError(cleanupFailures, "Continuity completed, but fixture cleanup failed");
}
