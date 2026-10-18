import { List, ListItem, Section, Time } from "@agentsfleet/design-system";
import { SCHEDULE_RUN_STATE } from "@/lib/api/draft-vocabulary";
import { AGENT_TRIGGER_TYPE, type FleetDetail } from "@/lib/types";
import { FleetTriggerSummary } from "./FleetTriggerSummary";

type Schedule = FleetDetail["schedules"][number];

export function FleetScheduleSummary({fleet}: {fleet: FleetDetail}) {
  const sources = fleet.triggers.filter((trigger) => trigger.type !== AGENT_TRIGGER_TYPE.cron);
  const requested = fleet.triggers.some((trigger) => trigger.type === AGENT_TRIGGER_TYPE.cron);
  return <Section aria-label="Schedule" className="gap-sm">
    <h3 className="text-label font-semibold text-muted-foreground">Schedule</h3>
    {fleet.schedules.length ? <List variant="plain" className="space-y-sm">
      {fleet.schedules.map((schedule) => <ListItem key={schedule.schedule_id} className="text-sm">
        <span className="block">{schedule.description}</span>
        <span className="block text-muted-foreground"><ScheduleState schedule={schedule} /></span>
      </ListItem>)}
    </List> : <p className="text-sm">{requested ? "Schedule saved; activation is not confirmed." : "No schedule set."}</p>}
    {sources.map((trigger, index) => <p key={index} className="text-sm"><FleetTriggerSummary trigger={trigger} /></p>)}
  </Section>;
}

function ScheduleState({schedule}: {schedule: Schedule}) {
  if (schedule.run_state === SCHEDULE_RUN_STATE.paused) return <>Schedule paused.</>;
  if (schedule.run_state === SCHEDULE_RUN_STATE.activationPending) return <>Schedule saved; activation is pending.</>;
  if (schedule.run_state === SCHEDULE_RUN_STATE.unavailable) return <>Next run time unavailable.</>;
  if (schedule.run_state !== SCHEDULE_RUN_STATE.scheduled) return <>Schedule status unavailable.</>;
  return schedule.next_run_at === null ? <>Next run time unavailable.</> : <>Next run: <Time value={schedule.next_run_at} format="relative" />.</>;
}
