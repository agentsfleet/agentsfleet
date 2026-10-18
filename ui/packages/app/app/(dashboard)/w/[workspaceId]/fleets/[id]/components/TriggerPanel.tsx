import type { FleetTrigger } from "@/lib/types";
import {
  Badge,
  DashboardPanelTitle,
  EmptyState,
  List,
  ListItem,
  Section,
  Time,
} from "@agentsfleet/design-system";
import { ZapIcon } from "lucide-react";
import { AGENT_TRIGGER_TYPE } from "@/lib/types";
import { triggerKey } from "./trigger-key";
import { FleetTriggerSummary } from "@/components/domain/FleetTriggerSummary";

export { triggerKey } from "./trigger-key";

type Props = {
  triggers?: FleetTrigger[];
  lastDeliveryByKey?: Record<string, string | null | undefined>;
};

const TRIGGERS_TITLE = "Configured triggers";
const TRIGGERS_EMPTY_TITLE = "Runs when you send a message";
const TRIGGERS_EMPTY_DESCRIPTION =
  "No automatic triggers or schedules are configured for this fleet.";

export default function TriggerPanel({ triggers = [], lastDeliveryByKey }: Props) {
  return (
    <Section aria-label={TRIGGERS_TITLE} className="gap-xl">
        <DashboardPanelTitle>{TRIGGERS_TITLE}</DashboardPanelTitle>
        {triggers.length === 0 ? (
          <EmptyState
            icon={<ZapIcon size={28} />}
            title={TRIGGERS_EMPTY_TITLE}
            description={TRIGGERS_EMPTY_DESCRIPTION}
          />
        ) : (
          <List variant="ordered" className="flex list-none flex-col gap-sm space-y-0 pl-0">
            {triggers.map((trigger) => {
              const key = triggerKey(trigger);
              return (
                <ListItem key={key}>
                  <TriggerRow
                    trigger={trigger}
                    lastDeliveryAt={lastDeliveryByKey?.[key]}
                  />
                </ListItem>
              );
            })}
          </List>
        )}
    </Section>
  );
}

function TriggerRow({
  trigger,
  lastDeliveryAt,
}: {
  trigger: FleetTrigger;
  lastDeliveryAt: string | null | undefined;
}) {
  return (
    <div className="flex flex-col gap-sm rounded-md border border-border p-md sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <p className="font-sans text-sm text-foreground">{triggerLabel(trigger)}</p>
        <p className="mt-xs break-words text-sm text-muted-foreground">
          <FleetTriggerSummary trigger={trigger} />
        </p>
      </div>
      <LastDelivery at={lastDeliveryAt} />
    </div>
  );
}

function triggerLabel(trigger: FleetTrigger): string {
  switch (trigger.type) {
    case AGENT_TRIGGER_TYPE.webhook:
      return "Webhook";
    case AGENT_TRIGGER_TYPE.cron:
      return "Schedule";
    case AGENT_TRIGGER_TYPE.mention:
      return "Chat mention";
  }
}

function LastDelivery({ at }: { at: string | null | undefined }) {
  if (at === undefined) return <Badge variant="default">Delivery history unavailable</Badge>;
  if (at === null) {
    return (
      <Badge variant="default" title="No recorded delivery has reached this trigger yet.">
        No deliveries yet
      </Badge>
    );
  }
  return (
    <Badge variant="default">
      Last delivery&nbsp;
      <Time value={at} format="relative" tooltip={false} />
    </Badge>
  );
}
