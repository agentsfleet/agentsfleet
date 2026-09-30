import type { WorkspaceMember } from "@/lib/api/tenant-members";
import { baseActorOf, isSteerBy, senderLabelFor } from "./event-summary";

// Who a thread's messages came from, by name. The viewer's own read "You"; a
// member of the fleet's account reads their display name, matched on the actor
// their messages record (`GET /v1/workspaces/{id}/members` names it); anyone
// else keeps the label `senderLabelFor` gives, which never renders an account
// identifier.

/** What a person's own messages read as. */
export const OWN_SENDER = "You";

/** One member's name, keyed by the actor their messages record. */
export type SenderName = { actor: string; name: string };

/** The viewer's subject, and each named member by actor. */
export type SenderNames = {
  viewer: string | null;
  byActor: ReadonlyMap<string, string>;
};

/** Names for a thread that knows no one: every label is today's fallback. */
export const NO_SENDER_NAMES: SenderNames = { viewer: null, byActor: new Map() };

/** The members a thread can name: those with a display name. */
export function namedMembers(members: readonly WorkspaceMember[]): SenderName[] {
  return members.flatMap(({ actor, display_name }) => {
    const name = (display_name ?? "").trim();
    return name.length > 0 ? [{ actor, name }] : [];
  });
}

export function senderNamesFrom(viewer: string | null, names: readonly SenderName[]): SenderNames {
  return { viewer, byActor: new Map(names.map(({ actor, name }) => [actor, name])) };
}

/** The label for `actor`'s messages in a thread on the fleet named `fleetName`. */
export function nameSender(actor: string, fleetName: string, names: SenderNames): string {
  const base = baseActorOf(actor);
  if (isSteerBy(base, names.viewer)) return OWN_SENDER;
  return names.byActor.get(base) ?? senderLabelFor(actor, fleetName);
}

/** Whether `actor` is a teammate the thread names: someone else, by name. Only
 * then is a label worth showing; every other turn keeps its chrome-free row. */
export function isNamedTeammate(actor: string, names: SenderNames): boolean {
  const base = baseActorOf(actor);
  return !isSteerBy(base, names.viewer) && names.byActor.has(base);
}
