"use client";

import { resolvePeopleAction } from "@/app/actions/identity";
import { isPersonId } from "./person";

/**
 * One lookup per paint, one answer per subject, for the whole page.
 *
 * A settled approvals table asks about every row it renders, and the same
 * operator usually decided most of them. Left to itself that is one request per
 * cell for a handful of distinct answers. So every `<PersonLabel>` registers its
 * subject here instead of fetching: the registrations for a render pass are
 * collected in a microtask, asked as one batch, and cached for the life of the
 * tab. A name does not change while somebody is looking at a table.
 *
 * Module state rather than a provider, deliberately — the cache is per tab, not
 * per subtree, and a label deep inside a dialog should not pay for a second
 * round-trip because nobody wrapped it.
 */

const names = new Map<string, string>();
const queued = new Set<string>();
const listeners = new Set<() => void>();
let scheduled = false;

/** The name held for a subject, or undefined while nobody has asked yet. */
export function nameFor(actor: string): string | undefined {
  return names.get(actor);
}

/** Register a subject for the next batch. Idempotent, and cheap to call in render. */
export function requestName(actor: string): void {
  if (!isPersonId(actor) || names.has(actor) || queued.has(actor)) return;
  queued.add(actor);
  if (scheduled) return;
  scheduled = true;
  queueMicrotask(() => void flush());
}

export function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

async function flush(): Promise<void> {
  scheduled = false;
  const asking = [...queued];
  queued.clear();
  if (asking.length === 0) return;
  const resolved: Record<string, string> = await resolvePeopleAction(asking).catch(
    () => ({}),
  );
  // Subjects the directory did not answer for are recorded as unknown rather
  // than left open, so a deleted account is asked about once and not on every
  // subsequent render of the same table.
  for (const actor of asking) names.set(actor, resolved[actor] ?? "");
  for (const listener of listeners) listener();
}

/** Test seam: the cache outlives a component, so a suite has to be able to clear it. */
export function resetPersonDirectory(): void {
  names.clear();
  queued.clear();
  listeners.clear();
  scheduled = false;
}
