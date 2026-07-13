"use client";

import { useMemo, useState } from "react";
import { ExternalLinkIcon } from "lucide-react";
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  cn,
  CopyButton,
  EYEBROW_CLASS,
  Input,
  Label,
  Time,
} from "@agentsfleet/design-system";
import type { FleetTrigger } from "@/lib/types";
import type { GuidanceCard } from "./provider-guidance";

type Props = {
  trigger: Extract<FleetTrigger, { type: "webhook" }>;
  webhookUrl: string;
  guidance: GuidanceCard;
  lastDeliveryAt?: number | null;
};

// Copy-feedback reset delay — how long a "Copied" affordance stays before
// reverting. Shared with TriggerPanel's CopyUrlFallback so the two copy
// surfaces in this feature can't drift apart.

export default function GuidedTriggerCard({
  trigger,
  webhookUrl,
  guidance,
  lastDeliveryAt,
}: Props) {
  const [vars, setVars] = useState<Record<string, string>>({});
  const events = useMemo(() => trigger.events ?? [], [trigger.events]);
  const command = useMemo(
    () => guidance.command(vars, webhookUrl, events),
    [guidance, vars, webhookUrl, events],
  );
  const deepLink = useMemo(() => guidance.webUiDeepLink(vars), [guidance, vars]);


  return (
    <Card data-testid={`guided-trigger-card-${trigger.source}`} className="bg-card">
      <CardHeader className="gap-1">
        <CardTitle className="text-base">{guidance.title}</CardTitle>
        <p className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
          {guidance.eventsLabel(events)}
        </p>
      </CardHeader>

      <CardContent className="flex flex-col gap-4">
        <CopyableLine label="Webhook URL" value={webhookUrl} testId="webhook-url" />

        {guidance.variables.length > 0 ? (
          <div className="flex flex-col gap-2">
            <span className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
              Variables
            </span>
            <div className="grid gap-2 sm:grid-cols-2">
              {guidance.variables.map((variable) => (
                <div key={variable.name} className="flex flex-col gap-1">
                  <Label
                    htmlFor={`var-${trigger.source}-${variable.name}`}
                    className="font-mono text-xs text-muted-foreground"
                  >
                    {variable.name}
                  </Label>
                  <Input
                    id={`var-${trigger.source}-${variable.name}`}
                    placeholder={variable.example}
                    value={vars[variable.name] ?? ""}
                    onChange={(e) =>
                      setVars((prev) => ({ ...prev, [variable.name]: e.target.value }))
                    }
                    aria-label={variable.name}
                  />
                </div>
              ))}
            </div>
          </div>
        ) : null}

        <div className="flex flex-col gap-2">
          <span className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
            Registration command
          </span>
          <pre
            data-testid={`command-${trigger.source}`}
            className="overflow-x-auto rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs leading-relaxed whitespace-pre"
          >
            {command}
          </pre>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          <CopyButton value={command} label="Copy registration command" showLabel />
          <CopyButton value={webhookUrl} label="Copy webhook URL" showLabel />
          <Button asChild variant="ghost" size="sm">
            <a
              href={deepLink}
              target="_blank"
              rel="noreferrer"
              aria-label={`Open ${guidance.title} in a new tab`}
            >
              <ExternalLinkIcon size={14} />
              Open {guidance.title} →
            </a>
          </Button>
        </div>

        <LastDelivery at={lastDeliveryAt ?? null} />
      </CardContent>
    </Card>
  );
}

function CopyableLine({
  label,
  value,
  testId,
}: {
  label: string;
  value: string;
  testId?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <span className={cn(EYEBROW_CLASS, "text-muted-foreground")}>
        {label}
      </span>
      <div className="flex items-center gap-2">
        <code
          data-testid={testId}
          className="flex-1 break-all rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs"
        >
          {value}
        </code>
        <CopyButton value={value} label={`Copy ${label}`} />
      </div>
    </div>
  );
}

function LastDelivery({ at }: { at: number | null }) {
  if (at == null) {
    return (
      <p className="font-mono text-xs text-muted-foreground" data-testid="last-delivery">
        Last delivery: never
      </p>
    );
  }
  return (
    <p className="font-mono text-xs text-muted-foreground" data-testid="last-delivery">
      Last delivery: <Time value={new Date(at)} format="relative" tooltip={false} />
    </p>
  );
}
