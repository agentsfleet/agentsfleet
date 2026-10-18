import { List, ListItem, Section } from "@agentsfleet/design-system";
import { SCHEDULE_SYNC } from "@/lib/api/draft-types";
import { AGENT_TRIGGER_TYPE, type FleetDetail } from "@/lib/types";
import { FleetTriggerSummary } from "./FleetTriggerSummary";

const SCHEDULE_STATUS = {active: "active", paused: "paused", deleting: "deleting"} as const;
type Schedule = FleetDetail["schedules"][number];

export function FleetScheduleSummary({fleet}: {fleet: FleetDetail}) {
  const sources = fleet.triggers.filter((trigger) => trigger.type !== AGENT_TRIGGER_TYPE.cron);
  const requested = fleet.triggers.some((trigger) => trigger.type === AGENT_TRIGGER_TYPE.cron);
  return <Section aria-label="Schedule" className="gap-sm">
    <h3 className="text-label font-semibold text-muted-foreground">Schedule</h3>
    {fleet.schedules.length ? <List variant="plain" className="space-y-sm">
      {fleet.schedules.map((schedule) => <ListItem key={schedule.schedule_id} className="text-sm">
        {scheduleState(schedule)} <span className="text-muted-foreground">Timezone: {schedule.timezone}.</span>
      </ListItem>)}
    </List> : <p className="text-sm">{requested ? "Schedule saved; activation is not confirmed." : "No schedule set."}</p>}
    {sources.map((trigger, index) => <p key={index} className="text-sm"><FleetTriggerSummary trigger={trigger} /></p>)}
  </Section>;
}

function scheduleState(schedule: Schedule): string {
  if (schedule.status === SCHEDULE_STATUS.paused) return "Schedule paused.";
  if (schedule.status === SCHEDULE_STATUS.deleting) return "Schedule is being removed.";
  if (schedule.sync !== SCHEDULE_SYNC.synced) return "Schedule saved, but activation is not confirmed.";
  if (schedule.status !== SCHEDULE_STATUS.active) return "Schedule status unavailable.";
  return "Schedule set. Next run time unavailable.";
}
