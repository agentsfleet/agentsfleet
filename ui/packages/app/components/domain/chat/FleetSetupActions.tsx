"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { Review } from "@/lib/api/draft-types";
import { useId, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, Input, Label } from "@agentsfleet/design-system";
import { DRAFT_STATE, START_KIND, isRunning } from "@/lib/api/draft-types";

type Props = {client: DraftClient; view: DraftView; setup: Review; dirty: boolean};
const ignore = (): void => undefined;

export function FleetSetupActions({client, view, setup, dirty, onSaved}: Props & {onSaved: () => void}) {
  const inputId = useId();
  const [message, setMessage] = useState("");
  const disabled = dirty || view.busy || view.pending || view.messages.some(isRunning) || view.draft?.state !== DRAFT_STATE.editing;
  return <div className="space-y-md">
    {dirty ? <p className="text-label text-warning">Update instructions before saving or publishing these changes.</p> : null}
    {setup.effective.target_setup_revision === undefined ? <div className="space-y-sm">
        <p className="text-label text-muted-foreground">Save creates your fleet. Configured automatic triggers become active when setup is ready.</p>
        <div className="space-y-xs"><Label htmlFor={inputId}>Optional first run</Label><Input id={inputId} placeholder="What should the saved fleet do first?" value={message} onChange={(event) => setMessage(event.target.value)} /></div>
        <Button variant="secondary" disabled={disabled || !message.trim()} onClick={() => {void client.save({kind: START_KIND.runNow, message: message.trim()}).then(onSaved).catch(ignore);}}>Save and run now</Button>
      </div> : null}
    <Accordion type="single" collapsible><AccordionItem value="publish"><AccordionTrigger>Publish to Fleet library</AccordionTrigger>
      <AccordionContent><PublicationActions client={client} view={view} setup={setup} dirty={dirty} /></AccordionContent>
    </AccordionItem></Accordion>
  </div>;
}

function PublicationActions({client, view, setup, dirty}: Props) {
  const inputId = useId();
  const [label, setLabel] = useState(setup.effective.instance_name);
  const disabled = dirty || view.busy || view.pending || view.messages.some(isRunning) || view.draft?.state !== DRAFT_STATE.editing;
  return <div className="space-y-sm">
    <p className="text-label text-muted-foreground">Publish these instructions as a versioned setup. Your conversation stays private. Existing fleets keep their setup.</p>
    <div className="space-y-xs"><Label htmlFor={inputId}>Library name</Label><Input id={inputId} value={label} onChange={(event) => setLabel(event.target.value)} maxLength={100} /></div>
    <div className="flex flex-wrap gap-sm"><Button variant="outline" disabled={disabled || !label.trim()} onClick={() => {void client.publish(label.trim(), false).catch(ignore);}}>Publish new entry</Button>
      {view.draft?.publication ? <Button variant="secondary" disabled={disabled} onClick={() => {void client.publish(label.trim(), true).catch(ignore);}}>Publish next revision</Button> : null}
      {client.canStagePlatform ? <Button variant="outline" disabled={disabled || !label.trim()} onClick={() => {void client.stagePlatform(label.trim()).catch(ignore);}}>Stage platform entry</Button> : null}</div>
  </div>;
}
