"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { ReactNode, RefObject } from "react";
import { useEffect, useRef, useState } from "react";
import Link from "next/link";
import { ArrowLeftIcon, BookOpenIcon, NotePencilIcon, StackIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuTrigger, Input, PageTitle, cn } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { chatTitle, firstChatRequest } from "@/lib/api/draft-summary";
import { DRAFT_STATE } from "@/lib/api/draft-types";

const SAVED_TINT_MS = 700;
type Props = {client: DraftClient; view: DraftView; workspace: string; fleetPage?: boolean; returnToFleet?: string; menuItems?: ReactNode};

export function DraftChatHeading(props: Props) {
  return props.view.inaccessible ? null : <HeadingContent key={props.view.draft?.id} {...props} />;
}

function HeadingContent({client, view, workspace, fleetPage, returnToFleet, menuItems}: Props) {
  const draft = view.draft;
  const empty = !draft && !view.preview && !view.fleet;
  const editing = Boolean(draft && draft.state === DRAFT_STATE.editing && !savedDraft(view));
  const locked = view.busy || view.pending || chatRecovery(view).pending;
  const privateDraft = editing && !view.fleet && !fleetPage ? draft : null;
  const draftTitle = draft ? chatTitle(draft.title, draft.id, firstChatRequest(view.messages) || draft.candidate?.definition.description) : null;
  const label = view.fleet?.name ?? (privateDraft ? draftTitle : draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? draftTitle) ?? "New fleet";
  const returnFleet = fleetPage ? undefined : view.fleet?.id ?? returnToFleet;
  const Title = empty || fleetPage ? "h2" : "h1";
  const Icon = empty ? NotePencilIcon : StackIcon;
  return <header className="fleet-chat-heading flex min-h-16 w-full shrink-0 items-center gap-lg border-b border-border-subtle px-lg summary-medium:px-xl summary-wide:px-2xl">
    <div className="flex min-w-0 flex-1 items-center gap-lg">
      <Icon data-app-header-icon="" size={16} className="hidden shrink-0 text-muted-foreground summary:block" aria-hidden="true" />
      {privateDraft ? <EditableDraftTitle client={client} draftId={privateDraft.id} name={label} locked={locked} />
        : <PageTitle as={Title} data-app-header-title="" size="chat" className="truncate font-semibold">{label}</PageTitle>}
      {editing || view.preview && !fleetPage && !view.savedSetupRevision ? <span className="inline-flex shrink-0 items-center gap-sm text-eyebrow text-muted-foreground"><span aria-hidden="true" className="size-1.5 rounded-full bg-current" />Draft</span> : null}
    </div>
    {returnFleet ? <Button asChild variant="ghost" size="icon" aria-label="Back to fleet conversation"><Link href={`/w/${workspace}/fleets/${returnFleet}`}><ArrowLeftIcon size={16} aria-hidden="true" /></Link></Button> : null}
    {empty ? <Button variant="ghost" size="icon-header" aria-label="New fleet" disabled={locked} onClick={() => client.fresh()}><NotePencilIcon size={16} aria-hidden="true" /></Button> : null}
    {menuItems ? <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="sm"><BookOpenIcon size={16} aria-hidden="true" />Memory</Button></DropdownMenuTrigger><DropdownMenuContent align="end">{menuItems}</DropdownMenuContent></DropdownMenu> : null}
  </header>;
}

type EditableTitleProps = {client: DraftClient; draftId: string; name: string; locked: boolean};
type TitleState = {kind: "viewing" | "saved"} | {kind: "editing" | "saving"; original: string; text: string};

function useTitleFocus(editing: boolean, input: RefObject<HTMLInputElement | null>,
  heading: RefObject<HTMLHeadingElement | null>, restoreRef: RefObject<boolean>) {
  useEffect(() => {
    if (editing) {
      const field = input.current;
      field?.focus();
      field?.setSelectionRange(field.value.length, field.value.length);
    } else if (restoreRef.current) {
      restoreRef.current = false;
      heading.current?.focus();
    }
  }, [editing, input, heading, restoreRef]);
}

function useTitleEditor({client, draftId, name, locked}: EditableTitleProps) {
  const [state, setState] = useState<TitleState>({kind: "viewing"});
  const input = useRef<HTMLInputElement>(null);
  const heading = useRef<HTMLHeadingElement>(null);
  const pending = useRef(false);
  const focusHeading = useRef(false);
  const text = "text" in state ? state.text : null;
  const saved = state.kind === "saved";
  useTitleFocus(text !== null, input, heading, focusHeading);
  useEffect(() => {
    if (!saved) return;
    const timer = setTimeout(() => setState({kind: "viewing"}), SAVED_TINT_MS);
    return () => clearTimeout(timer);
  }, [saved]);

  function begin() {
    if (locked) return;
    setState({kind: "editing", text: name, original: name});
  }

  function end(restoreFocus: boolean, kind: "viewing" | "saved" = "viewing") {
    focusHeading.current = restoreFocus;
    setState({kind});
  }

  function commit(restoreFocus: boolean) {
    if (state.kind !== "editing" || pending.current || locked) return;
    const next = state.text.trim();
    if (!next || next === state.original || next === name) { end(restoreFocus); return; }
    pending.current = true;
    setState({...state, kind: "saving"});
    void client.rename(draftId, next).then(
      () => end(restoreFocus && document.activeElement === input.current, "saved"),
      () => setState(state),
    ).finally(() => {pending.current = false;});
  }

  const setText = (next: string) => {
    if (state.kind === "editing") setState({...state, text: next});
  };
  return {text, setText, input, heading, pending, saving: state.kind === "saving", saved, begin, end, commit};
}

function EditableDraftTitle(props: EditableTitleProps) {
  const {name, locked} = props;
  const {text, setText, input, heading, pending, saving, saved, begin, end, commit} = useTitleEditor(props);
  const editing = text !== null;
  return <>
    <PageTitle ref={heading} data-app-header-title="" size="chat" hidden={editing}
      tabIndex={locked ? undefined : 0} title={locked ? undefined : "Click to edit the chat name"}
      className={cn("truncate rounded-sm font-semibold motion-safe:transition-colors focus-visible:outline-2 focus-visible:outline-ring",
        !locked && "cursor-text", saved && "text-success")}
      onClick={begin} onKeyDown={event => {
        if (event.nativeEvent.isComposing || !["Enter", " ", "F2"].includes(event.key)) return;
        event.preventDefault(); begin();
      }}>{name}</PageTitle>
    {editing ? <Input className="field-sizing-content min-h-0 w-auto min-w-0 max-w-full truncate border-0 bg-transparent p-0 text-chat-title leading-label font-semibold tracking-label text-muted-foreground caret-primary focus-visible:underline focus-visible:ring-0 focus-visible:ring-offset-0 pointer-coarse:min-h-0"
      ref={input} aria-label="Chat name" value={text} readOnly={locked || saving}
      onChange={event => setText(event.target.value)} onBlur={() => commit(false)}
      onKeyDown={event => {
        if (event.nativeEvent.isComposing || pending.current) return;
        if (event.key === "Escape") { event.preventDefault(); end(true); }
        if (event.key === "Enter") { event.preventDefault(); commit(true); }
      }} /> : null}
    <output className="sr-only">{saving ? "Saving chat name…" : saved ? "Chat name saved." : ""}</output>
  </>;
}
