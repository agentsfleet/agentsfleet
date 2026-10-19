import { AGENT_TRIGGER_TYPE, type FleetTrigger } from "@/lib/types";

export function FleetTriggerSummary({trigger}: {trigger: FleetTrigger}) {
  switch (trigger.type) {
    case AGENT_TRIGGER_TYPE.cron:
      return <>Schedule requested. Save the fleet to activate it.</>;
    case AGENT_TRIGGER_TYPE.webhook:
      return <>When {trigger.source} sends {trigger.events?.length ? trigger.events.join(", ") : "an event"} to this fleet’s webhook. Connect the source to enable deliveries.</>;
    case AGENT_TRIGGER_TYPE.mention:
      return <>When mentioned in {trigger.source}: {trigger.channels.join(", ")}. Connect the source to enable mentions.</>;
  }
}
