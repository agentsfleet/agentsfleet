import type { FleetTrigger } from "@/lib/types";

export const AGENT_TRIGGER_TYPE = {
  webhook: "webhook",
  cron: "cron",
  api: "api",
} as const;

export function triggerKey(t: FleetTrigger): string {
  switch (t.type) {
    case AGENT_TRIGGER_TYPE.webhook:
      return `${AGENT_TRIGGER_TYPE.webhook}:${t.source}`;
    case AGENT_TRIGGER_TYPE.cron:
      return `${AGENT_TRIGGER_TYPE.cron}:${t.schedule}`;
    case AGENT_TRIGGER_TYPE.api:
      return AGENT_TRIGGER_TYPE.api;
  }
}
