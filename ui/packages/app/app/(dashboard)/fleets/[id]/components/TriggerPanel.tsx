"use client";

import type { FleetTrigger } from "@/lib/types";
import { useEffect, useMemo, useState } from "react";
import { CheckIcon, CopyIcon } from "lucide-react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  Button,
  Card,
  CardContent,
  Time,
  useResettableTimeout,
} from "@agentsfleet/design-system";
import { webhookUrlFor } from "@/lib/api/fleets";
import GuidedTriggerCard, { COPY_RESET_MS } from "./GuidedTriggerCard";
import CronCard from "./CronCard";
import { guidanceFor } from "./provider-guidance";
import { AGENT_TRIGGER_TYPE, triggerKey } from "./trigger-key";

export { triggerKey } from "./trigger-key";

type Props = {
  fleetId: string;
  triggers?: FleetTrigger[];
  /**
   * Map of trigger-key → epoch-ms of the most-recent delivery for that trigger.
   * Keys are stable per-trigger ids produced by `triggerKey()`. `null` means
   * the parent looked but found no delivery; `undefined` means it did not
   * look at all (no badge rendered).
   */
  lastDeliveryByKey?: Record<string, number | null>;
};

function triggerLabel(t: FleetTrigger): string {
  switch (t.type) {
    case AGENT_TRIGGER_TYPE.webhook:
      return `Webhook · ${t.source}`;
    case AGENT_TRIGGER_TYPE.cron:
      return `Cron · ${t.schedule}`;
    case AGENT_TRIGGER_TYPE.api:
      return "API ingress";
  }
}

export default function TriggerPanel({
  fleetId,
  triggers,
  lastDeliveryByKey,
}: Props) {
  const list = useMemo(() => triggers ?? [], [triggers]);

  // Auto-expand the first trigger that has no recorded delivery — that's the
  // "still in setup" case the operator most wants to see. If every trigger
  // has delivered, leave the accordion collapsed by default.
  const initiallyOpen = useMemo(() => {
    if (list.length === 0) return undefined;
    const setup = list.find((t) => {
      const last = lastDeliveryByKey?.[triggerKey(t)];
      return last === null;
    });
    return setup ? triggerKey(setup) : undefined;
  }, [list, lastDeliveryByKey]);

  const [openValue, setOpenValue] = useState("");
  // Defer auto-expand to the client so server-rendered markup matches a collapsed accordion.
  useEffect(() => {
    setOpenValue(initiallyOpen ?? "");
  }, [initiallyOpen]);

  if (list.length === 0) {
    return <EmptyTriggers fleetId={fleetId} />;
  }

  return (
    <Card className="bg-card">
      <Accordion
        type="single"
        collapsible
        value={openValue}
        onValueChange={setOpenValue}
        data-testid="trigger-accordion"
      >
        {list.map((t) => {
          const key = triggerKey(t);
          const last = lastDeliveryByKey?.[key];
          return (
            <AccordionItem key={key} value={key} className="px-4 last:border-b-0">
              <AccordionTrigger className="font-mono text-sm">
                <span className="flex flex-1 items-center justify-between gap-3 pr-3">
                  <span data-testid={`trigger-label-${key}`}>{triggerLabel(t)}</span>
                  <LastDeliveryBadge at={last} />
                </span>
              </AccordionTrigger>
              <AccordionContent className="pb-4">
                <TriggerBody trigger={t} fleetId={fleetId} lastDeliveryAt={last} />
              </AccordionContent>
            </AccordionItem>
          );
        })}
      </Accordion>
      <CardContent className="text-xs text-muted-foreground">
        Edit <code className="font-mono">TRIGGER.md</code> and reinstall to change
        triggers — the source markdown is the source of truth.
      </CardContent>
    </Card>
  );
}

function TriggerBody({
  trigger,
  fleetId,
  lastDeliveryAt,
}: {
  trigger: FleetTrigger;
  fleetId: string;
  lastDeliveryAt: number | null | undefined;
}) {
  if (trigger.type === AGENT_TRIGGER_TYPE.cron) {
    return <CronCard trigger={trigger} fleetId={fleetId} />;
  }
  if (trigger.type === AGENT_TRIGGER_TYPE.webhook) {
    const guidance = guidanceFor(trigger.source);
    const url = webhookUrlFor(fleetId, trigger.source);
    if (!guidance) {
      return <CopyUrlFallback url={url} source={trigger.source} />;
    }
    return (
      <GuidedTriggerCard
        trigger={trigger}
        webhookUrl={url}
        guidance={guidance}
        lastDeliveryAt={lastDeliveryAt ?? null}
      />
    );
  }
  return <CopyUrlFallback url={webhookUrlFor(fleetId)} source={AGENT_TRIGGER_TYPE.api} />;
}

function LastDeliveryBadge({ at }: { at: number | null | undefined }) {
  if (at === undefined) return null;
  if (at === null) {
    return (
      <span
        className="font-mono text-xs text-muted-foreground"
        data-testid="last-delivery-badge"
      >
        never
      </span>
    );
  }
  return (
    <span
      className="font-mono text-xs text-muted-foreground"
      data-testid="last-delivery-badge"
    >
      <Time value={new Date(at)} format="relative" tooltip={false} />
    </span>
  );
}

// First-class sources get tailored copy. Everything else falls through to the
// generic "Unknown provider" line. `api` is a declared trigger type; `none`
// is the empty-triggers sentinel — both render via CopyUrlFallback as the
// rendering strategy, so calling them "Unknown provider" misleads operators.
const COPY_URL_FALLBACK_HELPER_TEXT: Record<string, string> = {
  api: "API ingress — POST events directly to this URL.",
  none: "Bare webhook URL — POST events here from any service.",
};

function CopyUrlFallback({ url, source }: { url: string; source: string }) {
  const [copied, setCopied] = useState(false);
  const resetTimer = useResettableTimeout();
  async function onCopy() {
    try {
      await navigator.clipboard.writeText(url);
      setCopied(true);
      resetTimer.start(() => setCopied(false), COPY_RESET_MS);
    } catch {
      // clipboard unavailable
    }
  }
  // Object.hasOwn guard — `source` can be operator-supplied via trigger config;
  // a bare bracket-access would inherit Object.prototype members (e.g.
  // `constructor`, `toString`) and render them as helper text.
  const helperText = Object.hasOwn(COPY_URL_FALLBACK_HELPER_TEXT, source)
    ? COPY_URL_FALLBACK_HELPER_TEXT[source]
    : "Unknown provider — paste this URL into any webhook-capable service.";
  return (
    <div className="flex flex-col gap-2" data-testid={`copy-url-fallback-${source}`}>
      <span className="font-mono text-label uppercase tracking-label text-muted-foreground">
        Webhook URL
      </span>
      <div className="flex items-center gap-2">
        <code
          data-testid="webhook-url"
          className="flex-1 break-all rounded-md border border-border bg-muted/30 px-3 py-2 font-mono text-xs"
        >
          {url}
        </code>
        <Button
          type="button"
          onClick={() => void onCopy()}
          variant="ghost"
          size="sm"
          aria-label="Copy webhook URL"
        >
          {copied ? <CheckIcon size={14} /> : <CopyIcon size={14} />}
          {copied ? "Copied" : "Copy"}
        </Button>
      </div>
      <p className="text-xs text-muted-foreground">{helperText}</p>
    </div>
  );
}

function EmptyTriggers({ fleetId }: { fleetId: string }) {
  return (
    <Card className="bg-card" data-testid="trigger-panel-empty">
      <CardContent className="flex flex-col gap-3 py-4">
        <p className="font-mono text-label uppercase tracking-label text-muted-foreground">
          No triggers declared
        </p>
        <p className="text-sm text-muted-foreground">
          Add <code className="font-mono">triggers:</code> entries to{" "}
          <code className="font-mono">TRIGGER.md</code> and reinstall to wire a
          webhook or cron trigger.
        </p>
        <CopyUrlFallback url={webhookUrlFor(fleetId)} source="none" />
      </CardContent>
    </Card>
  );
}
