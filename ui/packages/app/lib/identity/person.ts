/**
 * Who a stored "who did this" string names, and what to call them.
 *
 * Every surface that records a decision stores the caller's Clerk subject —
 * `user_3HizL5hdEfQ9Gy4e6Qsuq9nkKCu`. That is the right thing to store: it
 * survives a rename, and it is what agentsfleetd is handed. It is the wrong
 * thing to show. The daemon writes its own sentinels too, for the closures
 * nobody chose: the gate sweeper answers questions the deadline outlived.
 *
 * The name itself is not this module's job and never travels here. It arrives
 * on the row, joined from the deployment's own user table by the read that
 * fetched the subject. What is left is the vocabulary for the strings no user
 * table will ever hold — the sentinels — and what to print when the join came
 * back empty.
 */

/** Everything agentsfleetd attributes to itself rather than to a person. */
const SYSTEM_PREFIX = "system:";

/** What the daemon writes when the deadline, not a person, closed the gate. */
const SWEEPER = "system:approval_gate_sweeper";
const SWEEPER_LABEL = "Auto-swept";
const SYSTEM_LABEL = "System";

// Enough of the head to tell two subjects apart at a glance, enough of the
// tail to match one against a log line by eye.
const ID_HEAD = 10;
const ID_TAIL = 4;
const ELLIPSIS = "…";

/**
 * What the daemon calls itself, in words, or null when the actor is not one of
 * its sentinels. A name nobody can look up still has to read as somebody.
 */
export function systemLabel(actor: string): string | null {
  if (actor === SWEEPER) return SWEEPER_LABEL;
  if (actor.startsWith(SYSTEM_PREFIX)) return SYSTEM_LABEL;
  return null;
}

/** The id itself, trimmed to a width a table column can hold. */
export function shortenPersonId(actor: string): string {
  if (actor.length <= ID_HEAD + ID_TAIL + ELLIPSIS.length) return actor;
  return `${actor.slice(0, ID_HEAD)}${ELLIPSIS}${actor.slice(-ID_TAIL)}`;
}

/**
 * What to print before — or instead of — an answer from the directory.
 *
 * A subject Clerk cannot resolve (a deleted account, a directory that refused)
 * still gets shown, shortened rather than hidden: the row is a record of who
 * decided, and dropping the only evidence of that is worse than printing it
 * ugly. The full string is always one hover away.
 */
export function fallbackPersonLabel(actor: string): string {
  return systemLabel(actor) ?? shortenPersonId(actor);
}
