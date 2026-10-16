import type { FleetTrigger } from "@/lib/types";

import { AGENT_TRIGGER_TYPE } from "@/lib/types";

export function triggerKey(t: FleetTrigger): string {
  switch (t.type) {
    case AGENT_TRIGGER_TYPE.webhook:
      return `${AGENT_TRIGGER_TYPE.webhook}:${t.source}`;
    case AGENT_TRIGGER_TYPE.cron:
      return `${AGENT_TRIGGER_TYPE.cron}:${t.schedule}`;
    case AGENT_TRIGGER_TYPE.mention:
      return `${AGENT_TRIGGER_TYPE.mention}:${t.source}:${t.channels.join(",")}`;
  }
}
