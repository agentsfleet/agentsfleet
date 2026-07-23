"use client";

import { useState } from "react";
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
  CopyButton,
  TerminalPanel,
  cn,
} from "@agentsfleet/design-system";

const PAYLOAD_COPY_LABEL = "Copy JSON";
const PAYLOAD_TITLE = "Details";
const PAYLOAD_VALUE = "payload";

type FleetPayloadDisclosureProps = {
  json: string;
  /** Render the payload body directly inside a parent Details disclosure. */
  inline?: boolean;
};

/** Shows exactly the payload copied to the clipboard, including pretty JSON. */
export function FleetPayloadDisclosure({ json, inline = false }: FleetPayloadDisclosureProps) {
  const [isOpen, setIsOpen] = useState(false);
  if (inline) return <PayloadContent payload={formatPayload(json)} />;
  const payload = isOpen ? formatPayload(json) : null;
  return (
    <Accordion
      type="single"
      collapsible
      value={isOpen ? PAYLOAD_VALUE : ""}
      onValueChange={(value) => setIsOpen(value === PAYLOAD_VALUE)}
      className="mt-md"
    >
      <AccordionItem value={PAYLOAD_VALUE} className="border-0">
        <AccordionTrigger className="py-xs font-mono text-label text-muted-foreground hover:no-underline">
          {PAYLOAD_TITLE}
        </AccordionTrigger>
        <AccordionContent>
          {payload ? <PayloadContent payload={payload} /> : null}
        </AccordionContent>
      </AccordionItem>
    </Accordion>
  );
}

function PayloadContent({ payload }: { payload: string }) {
  return (
    <TerminalPanel
      title={PAYLOAD_TITLE}
      tag={<CopyButton value={payload} label={PAYLOAD_COPY_LABEL} />}
      className="mt-xs"
      bodyClassName="bg-surface-deep"
    >
      <pre className={cn("max-h-64 overflow-auto p-lg", "font-mono text-mono leading-mono text-foreground")}>
        {payload}
      </pre>
    </TerminalPanel>
  );
}

function formatPayload(json: string): string {
  try {
    const formatted = JSON.stringify(JSON.parse(json) as unknown, null, 2);
    return typeof formatted === "string" ? formatted : json;
  } catch {
    return json;
  }
}
