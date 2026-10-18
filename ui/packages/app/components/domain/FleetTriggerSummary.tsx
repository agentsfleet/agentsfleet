import { CronExpressionParser } from "cron-parser";
import { Time } from "@agentsfleet/design-system";
import { AGENT_TRIGGER_TYPE, type FleetTrigger } from "@/lib/types";

const DEFAULT_CRON_TIMEZONE = "UTC";

export function FleetTriggerSummary({trigger}: {trigger: FleetTrigger}) {
  switch (trigger.type) {
    case AGENT_TRIGGER_TYPE.cron:
      return <ScheduleSummary expression={trigger.schedule} timezone={trigger.timezone ?? DEFAULT_CRON_TIMEZONE} />;
    case AGENT_TRIGGER_TYPE.webhook:
      return <>When {trigger.source} sends {trigger.events?.length ? trigger.events.join(", ") : "an event"} to this fleet’s webhook. Connect the source to enable deliveries.</>;
    case AGENT_TRIGGER_TYPE.mention:
      return <>When mentioned in {trigger.source}: {trigger.channels.join(", ")}. Connect the source to enable mentions.</>;
  }
}

function ScheduleSummary({expression, timezone}: {expression: string; timezone: string}) {
  const next = nextScheduledTime(expression, timezone);
  return next
    ? <>On a schedule: {expression}. Next scheduled time: <Time value={next} format="relative" tooltip={false} />. Timezone: {timezone}. The schedule must be registered for automatic runs.</>
    : <>On a schedule: {expression}. Timing could not be previewed; check the saved schedule before relying on automatic runs.</>;
}

function nextScheduledTime(expression: string, timezone: string): Date | null {
  try {
    return CronExpressionParser.parse(expression, {tz: timezone}).next().toDate();
  } catch {
    return null;
  }
}
