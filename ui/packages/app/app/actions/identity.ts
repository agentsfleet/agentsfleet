"use server";

import { auth, clerkClient } from "@clerk/nextjs/server";
import { fallbackPersonLabel, isPersonId } from "@/lib/identity/person";

/**
 * Clerk subjects → the names people recognise.
 *
 * The dashboard stores and receives subjects; only Clerk knows the person. The
 * lookup is a server action because the secret that authorises it is a server
 * secret, and it answers a batch because a table of settled approvals asks
 * about every row at once.
 *
 * Scope: a signed-in caller only, capped per call. Every subject the client
 * asks about is one the server already rendered to it, so this widens what a
 * viewer can read from an id they hold to the name behind it — deliberately,
 * because a page that cannot say who approved something is not an audit trail.
 */

/** One page of Clerk's user list; a table asks about far fewer than this. */
const MAX_BATCH = 100;

export async function resolvePeopleAction(
  actors: string[],
): Promise<Record<string, string>> {
  const { userId } = await auth();
  if (!userId) return {};
  const wanted = [...new Set(actors.filter(isPersonId))].slice(0, MAX_BATCH);
  if (wanted.length === 0) return {};
  try {
    const clerk = await clerkClient();
    const { data } = await clerk.users.getUserList({
      userId: wanted,
      limit: wanted.length,
    });
    return Object.fromEntries(data.map((user) => [user.id, nameOf(user)]));
  } catch {
    // A directory that will not answer is not a page that should fail. The
    // caller keeps the shortened subject, which is what it was already showing.
    return {};
  }
}

type ClerkPerson = {
  id: string;
  fullName: string | null;
  username: string | null;
  primaryEmailAddress: { emailAddress: string } | null;
};

/**
 * The most human thing Clerk holds about a person: their name, else the handle
 * they chose, else the address they sign in with. An account with none of the
 * three is a subject with nothing to say, and reads as its own id.
 */
function nameOf(user: ClerkPerson): string {
  const full = user.fullName?.trim();
  if (full) return full;
  const handle = user.username?.trim();
  if (handle) return handle;
  const email = user.primaryEmailAddress?.emailAddress.trim();
  if (email) return email;
  return fallbackPersonLabel(user.id);
}
