// Server-only by construction: imported solely by a server component
// (fleets/page.tsx) and a "use server" action (actions/preferences.ts), both of
// which take a token this module never sources itself.
import { listFleets } from "@/lib/api/fleets";
import { listSecrets } from "@/lib/api/secrets";
import { listWorkspaceEvents } from "@/lib/api/events";
import { getTenantProvider } from "@/lib/api/tenant_provider";
import { getPreferences, prefIsTrue, PREFERENCE_KEY } from "@/lib/api/preferences";
import type { OnboardingInputs } from "@/lib/onboarding";

// Actor prefix that marks a steer event — the fleet is steered when the
// workspace has at least one event whose actor starts with this. Mirrors the
// server `actor_prefix` filter contract.
const STEER_ACTOR_PREFIX = "steer:";

// Gathers the six onboarding signals from endpoints that already exist — there
// is no onboarding-detection backend. Every read is independent and failure-
// tolerant: a signal that can't be read counts as not-done, which keeps its
// step visible. Fail-open toward SHOWING onboarding is the invariant (a read
// failure must never mark a step done and hide it). Runs server-side only; the
// browser holds no token.
//
// The six reads run CONCURRENTLY (one Promise.all), so wall-clock is one round
// trip, not six. `knownFleetTotal` lets a caller that already listed fleets (the
// Wall page) skip the fleet re-query entirely — one fewer API call on the empty
// wall, which is the one place the page and this gather both run.
export async function gatherOnboardingInputs(
  workspaceId: string,
  token: string,
  knownFleetTotal?: number,
): Promise<OnboardingInputs> {
  const [fleetTotal, secretCount, hasProcessedEvent, hasSteerEvent, modelConfigured, cliTicked] =
    await Promise.all([
      knownFleetTotal !== undefined
        ? Promise.resolve(knownFleetTotal)
        : listFleets(workspaceId, token, { limit: 1 })
            .then((p) => p.total)
            .catch(() => 0),
      listSecrets(workspaceId, token)
        .then((r) => r.secrets.length)
        .catch(() => 0),
      listWorkspaceEvents(workspaceId, token, { limit: 1 })
        .then((p) => p.items.length > 0)
        .catch(() => false),
      listWorkspaceEvents(workspaceId, token, { actor_prefix: STEER_ACTOR_PREFIX, limit: 1 })
        .then((p) => p.items.length > 0)
        .catch(() => false),
      getTenantProvider(token)
        .then((p) => p.model.trim().length > 0)
        .catch(() => false),
      getPreferences(workspaceId, token)
        .then((bag) => prefIsTrue(bag, PREFERENCE_KEY.CLI_TICKED))
        .catch(() => false),
    ]);

  return {
    modelConfigured,
    fleetTotal,
    secretCount,
    hasProcessedEvent,
    hasSteerEvent,
    cliTicked,
  };
}
