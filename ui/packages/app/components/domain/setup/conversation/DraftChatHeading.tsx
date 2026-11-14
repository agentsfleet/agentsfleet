"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useEffect, useId, useRef, useState, type ReactNode } from "react";
import { Button, Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle,
  DropdownMenu, DropdownMenuContent, DropdownMenuTrigger, Input, Label } from "@agentsfleet/design-system";
import Link from "next/link";
import { ArrowLeftIcon, BookOpenIcon, NotePencilIcon } from "@phosphor-icons/react/ssr";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { CHAT_TITLE_LIMIT, chatTitle, firstChatRequest } from "@/lib/api/draft-summary";
import nextDynamic from "next/dynamic";
import { EditSetupButton } from "../EditSetupControls";
import { AGENTSFLEET_STATUS } from "@/lib/api/fleets-types";

type Props = {client: DraftClient; view: DraftView; workspace: string; fleetPage?: boolean; returnToFleet?: string; menuItems?: ReactNode; onDirtyChange: (dirty: boolean) => void};
// The instructions editor is only needed after its dialog opens.
const DefinitionEditor = nextDynamic(() => import("../editor/DefinitionEditor").then(module => module.DefinitionEditor));
const INSTRUCTIONS = "Instructions";
const ignore = (): void => undefined;

export function DraftChatHeading(props: Props) {
  if (props.view.inaccessible) return null;
  return <HeadingContent {...props} />;
}

function HeadingContent({client, view, workspace, fleetPage, returnToFleet, menuItems, onDirtyChange}: Props) {
  const state = useHeadingState(client, view, onDirtyChange);
  const draft = view.draft;
  const recovery = chatRecovery(view);
  const locked = view.busy || view.pending || recovery.pending
    || Boolean(draft && draft.state !== DRAFT_STATE.editing && !savedDraft(view));
  const label = view.fleet?.name ?? draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? (draft ? chatTitle(draft.title, draft.id, firstChatRequest(view.messages) || draft.candidate?.definition.description) : "New fleet");
  const returnFleet = fleetPage ? undefined : view.fleet?.id ?? returnToFleet;
  const empty = !view.draft && !view.preview && !view.fleet;
  const canEdit = Boolean(draft?.candidate || view.preview || view.fleet?.definition);
  return <>
    <header className="fleet-chat-heading flex min-h-16 w-full shrink-0 flex-wrap items-center gap-md border-b border-border-subtle px-lg py-sm summary-medium:gap-lg summary-medium:px-xl summary-wide:px-2xl">
      {!fleetPage && !empty ? <Button asChild variant="ghost" size="icon" aria-label={returnFleet ? "Back to fleet conversation" : "Back to your fleets"}><Link href={returnFleet ? `/w/${workspace}/fleets/${returnFleet}` : `/w/${workspace}/fleets`}><ArrowLeftIcon size={16} aria-hidden="true" /></Link></Button> : null}
      {empty ? <NotePencilIcon data-app-header-icon="" size={16} className="hidden shrink-0 sm:block" aria-hidden="true" /> : null}
      <HeadingTitle client={client} view={view} state={state} label={label} locked={locked} primary={!fleetPage && Boolean(view.draft || view.preview || view.fleet)} />
      {view.fleet && fleetPage && view.fleet.status === AGENTSFLEET_STATUS.INSTALLING
        ? <EditSetupButton workspace={workspace} fleet={view.fleet.id} revision={view.fleet.setup_revision} /> : null}
      {draft && !canEdit ? <Button variant="ghost" size="sm" disabled={locked || draft.state !== DRAFT_STATE.editing} onClick={() => {state.setTitle(draft.title); state.setRenaming(true);}}>Rename chat</Button> : null}
      {canEdit ? <Button ref={state.trigger} variant="ghost" size="icon" aria-label="Edit instructions" title="Edit instructions" disabled={locked} onClick={() => {void state.prepare().catch(ignore);}}><NotePencilIcon size={16} aria-hidden="true" /></Button> : null}
      {empty ? <Button variant="ghost" size="icon-header" aria-label="New fleet" onClick={() => client.fresh()}><NotePencilIcon size={16} /></Button> : null}
      {menuItems ? <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="sm"><BookOpenIcon size={16} aria-hidden="true" />Memory</Button></DropdownMenuTrigger><DropdownMenuContent align="end">{menuItems}</DropdownMenuContent></DropdownMenu> : null}
    </header>
    <InstructionsDialog client={client} view={view} state={state} />
  </>;
}

type HeadingState = ReturnType<typeof useHeadingState>;

function useHeadingState(client: DraftClient, view: DraftView, onDirtyChange: Props["onDirtyChange"]) {
  const titleId = useId();
  const trigger = useRef<HTMLButtonElement>(null);
  const [renaming, setRenaming] = useState(false);
  const [title, setTitle] = useState("");
  const [editorOpen, setEditorOpen] = useState(false);
  const [dirty, setDirty] = useState(false);
  const empty = !view.draft && !view.preview && !view.fleet;
  useEffect(() => {
    if (empty) {setRenaming(false); setEditorOpen(false); setDirty(false); onDirtyChange(false);}
  }, [empty, onDirtyChange]);
  const changed = (value: boolean): void => {setDirty(value); onDirtyChange(value);};
  const closeEditor = (): void => {
    if (dirty && !window.confirm("Discard your unsaved instruction changes?")) return;
    setEditorOpen(false);
    changed(false);
  };
  const finishEditor = (): void => {changed(false); setEditorOpen(false);};
  const prepare = async (): Promise<void> => {
    if (savedDraft(client.getSnapshot())) await client.refreshSetup();
    setEditorOpen(true);
  };
  return {titleId, trigger, renaming, setRenaming, title, setTitle, editorOpen, dirty, changed, closeEditor, finishEditor, prepare};
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
  return <div className="min-w-0 flex-1"><Title data-app-header-title="" className="truncate text-chat-title font-semibold">{label}</Title>
    {view.draft || view.preview || view.fleet ? <p className="mt-xs text-eyebrow text-muted-foreground">Fleet setup{view.fleet ? ` · v${view.fleet.setup_revision}` : ""}</p> : null}</div>;
}

function InstructionsDialog({client, view, state}: {client: DraftClient; view: DraftView; state: HeadingState}) {
  const draft = view.draft;
  return <Dialog open={state.editorOpen} onOpenChange={state.closeEditor}>
    <DialogContent className="ph-no-capture ph-no-record bottom-0 left-auto right-0 top-0 flex h-dvh max-h-dvh w-full max-w-none translate-x-0 translate-y-0 flex-col rounded-none sm:max-w-form"
      onCloseAutoFocus={event => {event.preventDefault();if (state.renaming) document.getElementById(state.titleId)?.focus();else state.trigger.current?.focus();}}>
      <DialogHeader><DialogTitle>{INSTRUCTIONS}</DialogTitle><DialogDescription>Edit the instructions, then save your changes.</DialogDescription></DialogHeader>
      <DefinitionEditor client={client} view={view} onDirtyChange={state.changed} onUpdated={() => {state.finishEditor();}}
        onCancel={() => {state.finishEditor();}} />
      {draft ? <Button variant="link" size="eyebrow" className="self-start px-0" disabled={state.dirty || view.busy || view.pending || chatRecovery(view).pending || draft.state !== DRAFT_STATE.editing}
        onClick={() => {state.closeEditor();state.setTitle(draft.title);state.setRenaming(true);}}>Rename chat</Button> : null}
    </DialogContent>
  </Dialog>;
}
