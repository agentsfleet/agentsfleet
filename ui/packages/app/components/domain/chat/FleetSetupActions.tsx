"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { Review } from "@/lib/api/draft-types";
import { useId, useState } from "react";
import { Button, Collapsible, CollapsibleContent, CollapsibleTrigger, Input, Label } from "@agentsfleet/design-system";
import { DRAFT_STATE, isRunning } from "@/lib/api/draft-types";
import { useAuthoringIntent } from "./AuthoringIntent";
import { PluginPublicationTarget } from "./PluginPublicationTarget";

type Props = {client: DraftClient; view: DraftView; setup: Review; workspace: string; dirty: boolean; onPublish: () => void};
const ignore = (): void => undefined;

export function PublicationActions({client, view, setup, workspace, dirty, onPublish}: Props) {
  const {publicationTarget} = useAuthoringIntent();
  const inputId = useId();
  const [label, setLabel] = useState(setup.effective.instance_name);
  const [selected, setSelected] = useState<string | undefined>();
  const target = selected ?? publicationTarget ?? view.draft?.publication?.library_id;
  const disabled = dirty || view.busy || view.pending || view.messages.some(isRunning) || view.draft?.state !== DRAFT_STATE.editing;
  const publish = (append: boolean): void => {const request = client.publish(label.trim(), append, target); onPublish(); void request.catch(ignore);};
  return <div className="space-y-sm">
    <p className="text-label text-muted-foreground">Publish these instructions as a versioned setup. Your conversation stays private. Existing fleets keep their setup.</p>
    <div className="space-y-md"><Label htmlFor={inputId}>Plugin name</Label><Input id={inputId} value={label} onChange={(event) => setLabel(event.target.value)} maxLength={100} /></div>
    <div className="flex flex-wrap gap-sm"><Button variant="outline" disabled={disabled || !label.trim()} onClick={() => publish(false)}>Publish new entry</Button>
      {target ? <Button variant="outline" disabled={disabled} onClick={() => publish(true)}>Publish next revision</Button> : null}</div>
    {!publicationTarget && !view.draft?.publication ? <Collapsible>
      <CollapsibleTrigger asChild><Button variant="link" disabled={disabled}>Update an existing plugin</Button></CollapsibleTrigger>
      <CollapsibleContent className="pt-lg"><PluginPublicationTarget workspace={workspace} disabled={disabled} value={selected} onChange={setSelected} /></CollapsibleContent>
    </Collapsible> : null}
  </div>;
}
