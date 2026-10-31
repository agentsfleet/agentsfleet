"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Alert, Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
  DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuSeparator, DropdownMenuTrigger, Input, Label } from "@agentsfleet/design-system";
import { EllipsisIcon } from "lucide-react";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { CHAT_TITLE_LIMIT, chatTitle, firstChatRequest } from "@/lib/api/draft-summary";
import { DefinitionEditor } from "./DefinitionEditor";
import { PublicationActions } from "./FleetSetupActions";
import { DraftSaveButton } from "./DraftSaveBar";
import { EditSetupButton } from "./FleetSetupWorkspace";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";

type Props = {client: DraftClient; view: DraftView; workspace: string; fleetPage?: boolean; menuItems?: ReactNode; onDirtyChange: (dirty: boolean) => void};
const INSTRUCTIONS = "Instructions";
const PUBLISH = "Publish to Library";
const ignore = (): void => undefined;

export function DraftChatHeading(props: Props) {
  if (props.view.inaccessible) return null;
  return <HeadingContent {...props} />;
}

function HeadingContent({client, view, workspace, fleetPage, menuItems, onDirtyChange}: Props) {
  const state = useHeadingState(client, view, onDirtyChange);
  const draft = view.draft;
  const recovery = chatRecovery(view);
  const locked = view.busy || view.pending || recovery.pending || recovery.blocked
    || Boolean(draft && draft.state !== DRAFT_STATE.editing && !savedDraft(view));
  const label = view.fleet?.name ?? view.preview?.instanceName ?? (draft ? chatTitle(draft.title, draft.id, firstChatRequest(view.messages) || draft.candidate?.definition.description) : "New fleet");
  const canEdit = Boolean(draft?.candidate || view.preview || view.fleet?.definition);
  return <>
    <div className="fleet-chat-heading flex w-full shrink-0 flex-wrap items-center gap-md border-b border-border px-xl pb-xl pt-5 sm:px-chat-inset">
      <HeadingTitle client={client} view={view} state={state} label={label} locked={locked} primary={!fleetPage && Boolean(view.draft || view.preview || view.fleet)} />
      {view.fleet && fleetPage && view.fleet.status === AGENTSFLEET_STATUS.INSTALLING
        ? <EditSetupButton workspace={workspace} fleet={view.fleet.id} revision={view.fleet.setup_revision} /> : null}
      <DraftSaveButton client={client} view={view} dirty={state.dirty} />
      {canEdit || draft ? <HeadingMenu state={state} label={label} canEdit={canEdit} locked={locked} canRename={draft?.state === DRAFT_STATE.editing}>{menuItems}</HeadingMenu> : null}
    </div>
    <InstructionsDialog client={client} view={view} state={state} />
    <Dialog open={state.publishing} onOpenChange={state.setPublishing}>
      <DialogContent className="ph-no-capture ph-no-record"><DialogHeader><DialogTitle>{PUBLISH}</DialogTitle><DialogDescription>Only the saved setup is published when you confirm.</DialogDescription></DialogHeader>
        {view.error ? <Alert variant="destructive">{view.error}</Alert> : null}
        {view.notice ? <output className="text-label text-muted-foreground">{view.notice}</output> : null}
        {draft?.review ? <PublicationActions client={client} view={view} setup={draft.review} dirty={state.dirty} onPublish={() => state.setPublishing(false)} /> : null}
      </DialogContent>
    </Dialog>
  </>;
}

type HeadingState = ReturnType<typeof useHeadingState>;

function useHeadingState(client: DraftClient, view: DraftView, onDirtyChange: Props["onDirtyChange"]) {
  const titleId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [renaming, setRenaming] = useState(false);
  const [title, setTitle] = useState("");
  const [editorOpen, setEditorOpen] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [dirty, setDirty] = useState(false);
  const empty = !view.draft && !view.preview && !view.fleet;
  useEffect(() => {
    if (empty) {setRenaming(false); setEditorOpen(false); setPublishing(false); setDirty(false); onDirtyChange(false);}
  }, [empty, onDirtyChange]);
  const changed = (value: boolean): void => {setDirty(value); onDirtyChange(value);};
  const closeEditor = (): void => {
    if (dirty && !window.confirm("Discard your unsaved instruction changes?")) return;
    setEditorOpen(false);
    changed(false);
  };
  const finishEditor = (): void => {changed(false); setEditorOpen(false);};
  const prepare = async (publish: boolean): Promise<void> => {
    if (savedDraft(client.getSnapshot())) await client.refreshSetup();
    if (publish) {
      const current = client.getSnapshot();
      if (!current.draft && current.preview) {
        await client.edit({definition: current.preview.definition, instanceName: current.preview.instanceName});
        await client.refreshSetup();
      }
      setPublishing(true);
    } else setEditorOpen(true);
  };
  return {titleId, trigger, renaming, setRenaming, title, setTitle, editorOpen, publishing, setPublishing, dirty, changed, closeEditor, finishEditor, prepare};
}

function HeadingTitle({client, view, state, label, locked, primary}: {client: DraftClient; view: DraftView; state: HeadingState; label: string; locked: boolean; primary: boolean}) {
  const Title = primary ? "h1" : "h2";
  if (state.renaming && view.draft) return <>
    <Title className="sr-only">{label}</Title>
    <Label htmlFor={state.titleId} className="sr-only">Chat name</Label>
    <Input id={state.titleId} value={state.title} onChange={event => state.setTitle(event.target.value)} maxLength={CHAT_TITLE_LIMIT} className="min-w-0 flex-1" />
    <Button size="sm" disabled={locked || !state.title.trim()} onClick={() => {void client.rename(state.title.trim()).then(() => state.setRenaming(false)).catch(ignore);}}>Save name</Button>
    <Button variant="ghost" size="sm" disabled={locked} onClick={() => state.setRenaming(false)}>Cancel</Button>
  </>;
  return <div className="min-w-0 flex-1"><Title className="truncate text-chat-title font-semibold">{label}</Title>
    <p className="mt-xs text-eyebrow text-muted-foreground">Fleet setup{view.fleet ? ` · v${view.fleet.setup_revision}` : ""}</p></div>;
}

function HeadingMenu({state, label, canEdit, locked, canRename, children}: {state: HeadingState; label: string; canEdit: boolean; locked: boolean; canRename: boolean; children?: ReactNode}) {
  return <DropdownMenu><DropdownMenuTrigger asChild><Button ref={state.trigger} variant="ghost" size="icon-compact" aria-label="Setup chat actions"><EllipsisIcon aria-hidden="true" /></Button></DropdownMenuTrigger>
    <DropdownMenuContent align="end">
      <DropdownMenuItem disabled={locked || !canEdit} onSelect={() => {void state.prepare(false).catch(ignore);}}>{INSTRUCTIONS}</DropdownMenuItem>
      <DropdownMenuItem disabled={locked || !canEdit} onSelect={() => {void state.prepare(true).catch(ignore);}}>{PUBLISH}</DropdownMenuItem>
      <DropdownMenuSeparator />
      <DropdownMenuItem disabled={locked || !canRename} onSelect={() => {state.setTitle(label); state.setRenaming(true);}}>Rename chat</DropdownMenuItem>
      {children}
    </DropdownMenuContent>
  </DropdownMenu>;
}

function InstructionsDialog({client, view, state}: {client: DraftClient; view: DraftView; state: HeadingState}) {
  return <Dialog open={state.editorOpen} onOpenChange={state.closeEditor}>
    <DialogContent className="ph-no-capture ph-no-record bottom-0 left-auto right-0 top-0 flex h-dvh max-h-dvh w-full max-w-none translate-x-0 translate-y-0 flex-col rounded-none sm:max-w-form"
      onCloseAutoFocus={event => {event.preventDefault(); state.trigger.current?.focus();}}>
      <DialogHeader><DialogTitle>{INSTRUCTIONS}</DialogTitle><DialogDescription>Edit the instructions, then save your changes.</DialogDescription></DialogHeader>
      <DefinitionEditor client={client} view={view} onDirtyChange={state.changed} onUpdated={() => {state.finishEditor();}}
        onCancel={() => {state.finishEditor();}} />
    </DialogContent>
  </Dialog>;
}
