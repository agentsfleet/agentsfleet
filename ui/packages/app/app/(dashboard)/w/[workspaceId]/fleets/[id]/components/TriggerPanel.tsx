import type { FleetTrigger } from "@/lib/types";
import {
  Badge,
  Card,
  CardContent,
  EmptyState,
  List,
  ListItem,
  Time,
} from "@agentsfleet/design-system";
import { ZapIcon } from "lucide-react";
import { AGENT_TRIGGER_TYPE } from "@/lib/types";
import { triggerKey } from "./trigger-key";

export { triggerKey } from "./trigger-key";

type Props = {
  triggers?: FleetTrigger[];
  lastDeliveryByKey?: Record<string, string | null>;
};

const TRIGGERS_TITLE = "Configured triggers";
const TRIGGERS_EMPTY_TITLE = "Chat and steer messages only";
const TRIGGERS_EMPTY_DESCRIPTION =
  "Open Setup and improve this fleet to add a trigger.";

export default function TriggerPanel({ triggers = [], lastDeliveryByKey }: Props) {
  return (
    <Card className="bg-card" aria-label={TRIGGERS_TITLE}>
      <CardContent className="flex flex-col gap-md">
        <h2 className="font-sans text-sm font-medium">{TRIGGERS_TITLE}</h2>
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
        <p className="text-xs text-muted-foreground">
          Use Improve from Setup to change how this fleet wakes. Review and Apply affect new work.
        </p>
      </CardContent>
    </Card>
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
          {triggerDetail(trigger)}
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

function triggerDetail(trigger: FleetTrigger): string {
  switch (trigger.type) {
    case AGENT_TRIGGER_TYPE.webhook: {
      const events = trigger.events?.length ? ` · ${trigger.events.join(", ")}` : "";
      return `${trigger.source}${events}`;
    }
    case AGENT_TRIGGER_TYPE.cron:
      return trigger.schedule;
    case AGENT_TRIGGER_TYPE.mention:
      return `${trigger.source} · ${trigger.channels.join(", ")}`;
  }
}

function LastDelivery({ at }: { at: string | null | undefined }) {
  if (at === undefined) return null;
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
