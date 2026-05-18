import { listZombieEvents } from "@/lib/api/events";
import type { ZombieTrigger } from "@/lib/types";
import { triggerKey } from "./TriggerPanel";

/**
 * Maps a declared trigger to the actor-glob the events API recognises.
 * `api` triggers don't carry a stable actor namespace (every webhook
 * ingress shares the bare `/v1/webhooks/{id}` URL), so they opt out —
 * the caller surfaces `null` in the per-trigger map.
 */
export function actorGlobFor(t: ZombieTrigger): string | null {
  switch (t.type) {
    case "webhook":
      return `webhook:${t.source}:*`;
    case "cron":
      return `cron:*`;
    case "api":
      return null;
  }
}

/**
 * Per-trigger "last delivery" lookup. One lightweight server-side call
 * per declared trigger, in parallel; failures degrade to `null` so the
 * TriggerPanel renders the "never" badge. Webhook actors are namespaced
 * as `webhook:<source>:*`; cron as `cron:*`; api triggers always land
 * as `null` (no stable namespace).
 */
export async function resolveLastDeliveries(
  workspaceId: string,
  zombieId: string,
  token: string,
  triggers: readonly ZombieTrigger[],
): Promise<Record<string, number | null>> {
  const out: Record<string, number | null> = {};
  await Promise.all(
    triggers.map(async (t) => {
      const key = triggerKey(t);
      const actor = actorGlobFor(t);
      if (!actor) {
        out[key] = null;
        return;
      }
      try {
        const page = await listZombieEvents(workspaceId, zombieId, token, {
          actor,
          limit: 1,
        });
        out[key] = page.items[0]?.created_at ?? null;
      } catch {
        out[key] = null;
      }
    }),
  );
  return out;
}
