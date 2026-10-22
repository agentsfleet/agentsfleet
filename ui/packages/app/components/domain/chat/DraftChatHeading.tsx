"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useRef, useState } from "react";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Alert, Button, Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle,
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger, Input, Label } from "@agentsfleet/design-system";
import { EllipsisIcon } from "lucide-react";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { CHAT_TITLE_LIMIT, chatTitle, firstChatRequest } from "@/lib/api/draft-summary";
import { DefinitionEditor } from "./DefinitionEditor";
import { FleetSetupDetails } from "./FleetSetupDetails";
import { FleetSetupActions } from "./FleetSetupActions";
import { DraftSaveButton } from "./DraftSaveBar";

type Props = {client: DraftClient; view: DraftView};
const EDIT_LABEL = "Edit instructions";
const ignore = (): void => undefined;

export function DraftChatHeading({client, view}: Props) {
  if (view.inaccessible || (!view.draft && !view.preview)) return null;
  return <SetupChatHeading client={client} view={view} />;
}

function SetupChatHeading({client, view}: Props) {
  const [editing, setEditing] = useState(false);
  const [editorOpen, setEditorOpen] = useState(false);
  const [editorDirty, setEditorDirty] = useState(false);
  const [title, setTitle] = useState("");
  const trigger = useRef<HTMLButtonElement>(null);
  const draft = view.draft;
  const recovery = chatRecovery(view);
  const locked = view.busy || view.pending || recovery.pending || recovery.blocked || Boolean(draft && draft.state !== DRAFT_STATE.editing);
  const label = draft ? chatTitle(draft.title, draft.id, firstChatRequest(view.messages) || draft.candidate?.definition.description) : "Fleet setup";
  const canEdit = Boolean(draft?.candidate || view.preview);
  return <Dialog open={editorOpen} onOpenChange={setEditorOpen}>
    <div className="mx-auto flex w-full max-w-measure shrink-0 flex-wrap items-center gap-sm px-sm">
    {editing && draft ? <><Label htmlFor={`${draft.id}-title`} className="sr-only">Chat name</Label>
      <Input id={`${draft.id}-title`} value={title} onChange={(event) => setTitle(event.target.value)} maxLength={CHAT_TITLE_LIMIT} className="min-w-0 flex-1" />
      <Button size="sm" disabled={locked || !title.trim()} onClick={() => {void client.rename(title.trim()).then(() => setEditing(false)).catch(ignore);}}>Save name</Button>
      <Button variant="ghost" size="sm" disabled={locked} onClick={() => setEditing(false)}>Cancel</Button></>
      : <h2 className="min-w-0 flex-1 truncate text-body font-semibold">{label}</h2>}
    {draft?.review?.effective.target_setup_revision === undefined ? <DraftSaveButton client={client} view={view} /> : null}
    <DropdownMenu><DropdownMenuTrigger asChild><Button ref={trigger} variant="ghost" size="icon" aria-label="Setup chat actions"><EllipsisIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
      <DropdownMenuContent align="end" onCloseAutoFocus={(event) => {if (editorOpen) event.preventDefault();}}>
        <DropdownMenuItem disabled={locked || !canEdit} onSelect={() => setEditorOpen(true)}>{EDIT_LABEL}</DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem disabled={locked || !draft} onSelect={() => {setTitle(label); setEditing(true);}}>Rename chat</DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
    </div>
    <DialogContent className="ph-no-capture ph-no-record bottom-0 left-auto right-0 top-0 h-dvh max-h-dvh w-full max-w-none translate-x-0 translate-y-0 rounded-none sm:max-w-form"
      onCloseAutoFocus={(event) => {event.preventDefault(); trigger.current?.focus();}}>
      <DialogHeader><DialogTitle>{EDIT_LABEL}</DialogTitle><DialogDescription>Update the proposed instructions, then return to chat and save the fleet.</DialogDescription></DialogHeader>
      {view.error ? <Alert variant="destructive">{view.error}</Alert> : null}
      {view.notice ? <output className="text-label text-muted-foreground">{view.notice}</output> : null}
      <DefinitionEditor client={client} view={view} onDirtyChange={setEditorDirty} onUpdated={() => setEditorOpen(false)} />
      {draft?.review ? <Accordion type="single" collapsible><AccordionItem value="review"><AccordionTrigger>Review and run options</AccordionTrigger>
        <AccordionContent><FleetSetupDetails review={draft.review} /><FleetSetupActions client={client} view={view} setup={draft.review} dirty={editorDirty} onSaved={() => setEditorOpen(false)} /></AccordionContent>
      </AccordionItem></Accordion> : null}
      <DialogFooter><Button variant="ghost" onClick={() => setEditorOpen(false)}>Cancel editing</Button></DialogFooter>
    </DialogContent>
  </Dialog>;
}
