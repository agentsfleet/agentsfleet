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
import { AGENT_TRIGGER_TYPE, triggerKey } from "./trigger-key";

export { triggerKey } from "./trigger-key";

type Props = {
  triggers?: FleetTrigger[];
  lastDeliveryByKey?: Record<string, number | null>;
};

const TRIGGERS_TITLE = "Configured triggers";
const TRIGGERS_EMPTY_TITLE = "No triggers declared";
const TRIGGERS_EMPTY_DESCRIPTION =
  "Add a trigger declaration to TRIGGER.md. Saved changes take effect on the next wake.";

export default function TriggerPanel({ triggers = [], lastDeliveryByKey }: Props) {
  return (
    <Card className="bg-card" aria-label={TRIGGERS_TITLE}>
      <CardContent className="flex flex-col gap-md py-4">
        <h2 className="font-mono text-sm font-medium">{TRIGGERS_TITLE}</h2>
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
          Edit <code className="font-mono">TRIGGER.md</code> above to change how this fleet wakes.
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
  lastDeliveryAt: number | null | undefined;
}) {
  return (
    <div className="flex flex-col gap-sm rounded-md border border-border p-md sm:flex-row sm:items-center sm:justify-between">
      <div className="min-w-0">
        <p className="font-mono text-sm text-foreground">{triggerLabel(trigger)}</p>
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
    case AGENT_TRIGGER_TYPE.api:
      return "API ingress";
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
    case AGENT_TRIGGER_TYPE.api:
      return "Accepts events through the fleet API.";
  }
}

function LastDelivery({ at }: { at: number | null | undefined }) {
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
      <Time value={new Date(at)} format="relative" tooltip={false} />
    </Badge>
  );
}
