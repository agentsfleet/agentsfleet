const ACTOR_OPERATOR_PREFIX = "steer:";
const ACTOR_FLEET = "fleet";
const ACTOR_GITHUB_APP = "github-app";
const ACTOR_GITHUB_WEBHOOK = "webhook:github";
const ACTOR_CRON = "cron";
const ACTOR_CRON_PREFIX = "cron:";
const ACTOR_CONTINUATION = "continuation";
const ACTOR_CONTINUATION_PREFIX = "continuation:";

export function presentFleetActor(actor: string): string {
  const normalized = actor.trim().toLowerCase();
  if (normalized.startsWith(ACTOR_OPERATOR_PREFIX)) return "Operator";
  if (normalized === ACTOR_FLEET) return "Fleet";
  if (normalized === ACTOR_GITHUB_APP || normalized === ACTOR_GITHUB_WEBHOOK) return "GitHub App";
  if (normalized === ACTOR_CRON || normalized.startsWith(ACTOR_CRON_PREFIX)) return "Cron";
  if (
    normalized === ACTOR_CONTINUATION ||
    normalized.startsWith(ACTOR_CONTINUATION_PREFIX)
  ) return "Continuation";
  return actor;
}
