import type { FleetTrigger } from "@/lib/types";

export const AGENT_TRIGGER_TYPE = {
  webhook: "webhook",
  cron: "cron",
  api: "api",
  mention: "mention",
} as const;

export function triggerKey(t: FleetTrigger): string {
  switch (t.type) {
    case AGENT_TRIGGER_TYPE.webhook:
      return `${AGENT_TRIGGER_TYPE.webhook}:${t.source}`;
    case AGENT_TRIGGER_TYPE.cron:
      return `${AGENT_TRIGGER_TYPE.cron}:${t.schedule}`;
    case AGENT_TRIGGER_TYPE.api:
      return AGENT_TRIGGER_TYPE.api;
    case AGENT_TRIGGER_TYPE.mention:
      return `${AGENT_TRIGGER_TYPE.mention}:${t.source}:${t.channels.join(",")}`;
  }
}
