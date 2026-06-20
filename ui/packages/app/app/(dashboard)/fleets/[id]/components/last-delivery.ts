import { listFleetEvents } from "@/lib/api/events";
import type { FleetTrigger } from "@/lib/types";
import { AGENT_TRIGGER_TYPE, triggerKey } from "./trigger-key";

/**
 * Maps a declared trigger to the actor-glob the events API recognises.
 *
 * Cron triggers all share `cron:*` regardless of schedule — the server
 * does not tag cron events with the specific schedule that fired, so
 * the actor space has no per-schedule namespace to glob against. A
 * fleet with two cron triggers (e.g. every-15-min and hourly)
 * therefore renders the same "last delivery" timestamp on both cards
 * — whichever cron fired most recently. The right place to fix this
 * is the server's cron actor format, not the dashboard's glob.
 *
 * `api` triggers don't carry a stable actor namespace (every webhook
 * ingress shares the bare `/v1/webhooks/{id}` URL), so they opt out —
 * the caller surfaces `null` in the per-trigger map.
 */
export function actorGlobFor(t: FleetTrigger): string | null {
  switch (t.type) {
    case AGENT_TRIGGER_TYPE.webhook:
      return `${AGENT_TRIGGER_TYPE.webhook}:${t.source}:*`;
    case AGENT_TRIGGER_TYPE.cron:
      return `${AGENT_TRIGGER_TYPE.cron}:*`;
    case AGENT_TRIGGER_TYPE.api:
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
  fleetId: string,
  token: string,
  triggers: readonly FleetTrigger[],
): Promise<Record<string, number | null>> {
  const out: Record<string, number | null> = {};
  await Promise.all(
    triggers.map(async (t) => {
      const key = triggerKey(t);
      const actor = actorGlobFor(t);
      if (!actor) {
        // Leave the key absent — `undefined` reads as "parent did not
        // look" in TriggerPanel's prop semantics, which suppresses both
        // the "never" delivery badge and the auto-expand-on-mount path.
        // Writing `null` would falsely fire both on every api trigger.
        return;
      }
      try {
        const page = await listFleetEvents(workspaceId, fleetId, token, {
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
