/**
 * observation.ts — bounded reads of a live environment, and the vocabulary a
 * failed one prints.
 *
 * A journey against a deployed build spends most of its time WAITING: for a
 * runner to lease, for a provider to answer, for a ledger row to settle. Two
 * shapes cover all of it, and both live here rather than in each walk, because
 * an unbounded wait and a bare fetch failure are the two ways an acceptance
 * lane stops being able to say what broke.
 *
 * `pollFor` answers `null` when the budget runs out, so the CALLER classifies
 * the silence — a poll that threw its own timeout would report "the test timed
 * out" about an environment that was merely slow. `observed` wraps every API
 * read in the leg it belongs to, so a daemon that stops answering mid-journey
 * is named as such (`fixtures/execution.ts` owns the classifiers).
 *
 * Read `fixtures/execution.ts` for the verdict vocabulary these two produce.
 */
import { classifyApiFailure, failWith, type JourneyLeg } from "./execution";

/** How often a bounded poll re-reads. One read per two seconds is the cadence
 * every journey here already used; a tighter one just spends the environment's
 * request budget on the same answer. */
export const POLL_INTERVAL_MS = 2_000;

/** Enough of a UUID to tell two runs apart in a trace, short enough to ride a
 * fleet name the server may suffix. */
const TAG_LENGTH = 8;

/** A per-run tag, so a name or a message can be tied back to one walk. */
export function uniqueTag(): string {
  return crypto.randomUUID().slice(0, TAG_LENGTH);
}

/**
 * A bounded poll that answers `null` when the budget runs out.
 *
 * The budget is the caller's, and it is always named: an unbounded wait here
 * would surface as the suite's own blanket timeout, which says nothing about
 * which leg stalled.
 *
 * It is also how an ABSENCE is proved. A park — "no lease was issued while the
 * grant was pending" — is this same call read the other way round: a non-null
 * answer inside the window is the violation, and `null` is the proof. There is
 * no second helper for that, because it would be this one with its result
 * negated at a different call site.
 */
export async function pollFor<T>(
  read: () => Promise<T | null>,
  timeoutMs: number,
): Promise<T | null> {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const value = await read();
    if (value !== null) return value;
    if (Date.now() > deadline) return null;
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
}

/**
 * Every API read inside a walk goes through here, so a daemon that stops
 * answering mid-journey is named with the leg it broke on rather than
 * surfacing as a bare fetch failure.
 */
export async function observed<T>(leg: JourneyLeg, read: () => Promise<T>): Promise<T> {
  try {
    return await read();
  } catch (error) {
    return failWith(classifyApiFailure(error, leg));
  }
}
