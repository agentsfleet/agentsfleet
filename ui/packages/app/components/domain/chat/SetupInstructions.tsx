"use client";

import { useCallback, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DefinitionEditor } from "./DefinitionEditor";
import { FleetSetupDetails } from "./FleetSetupDetails";
import { FleetSetupActions } from "./FleetSetupActions";

export function SetupInstructions({client, view, onDirtyChange}: {client: DraftClient; view: DraftView; onDirtyChange: (dirty: boolean) => void}) {
  const [open, setOpen] = useState("");
  const [opened, setOpened] = useState(false);
  const [dirty, setDirty] = useState(false);
  const dirtyChanged = useCallback((value: boolean) => {setDirty(value); onDirtyChange(value);}, [onDirtyChange]);
  const updated = (): void => {dirtyChanged(false); setOpen(""); setOpened(false);};
  return <Accordion type="single" collapsible value={open} onValueChange={value => {setOpen(value); if (value) setOpened(true);}}>
    <AccordionItem value="instructions" className="border-0">
      <AccordionTrigger className="w-fit gap-sm py-md text-label text-muted-foreground">Edit instructions manually</AccordionTrigger>
      {opened ? <AccordionContent forceMount hidden={!open}>
        <DefinitionEditor client={client} view={view} onDirtyChange={dirtyChanged} onUpdated={updated} />
        {view.draft?.review ? <Accordion type="single" collapsible><AccordionItem value="review"><AccordionTrigger>Review and publish options</AccordionTrigger>
          <AccordionContent><FleetSetupDetails review={view.draft.review} /><FleetSetupActions client={client} view={view} setup={view.draft.review} dirty={dirty} onSaved={updated} /></AccordionContent>
        </AccordionItem></Accordion> : null}
      </AccordionContent> : null}
    </AccordionItem>
  </Accordion>;
}
