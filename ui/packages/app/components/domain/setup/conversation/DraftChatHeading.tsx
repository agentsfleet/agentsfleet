"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { FormEvent, ReactNode } from "react";
import { useEffect, useId, useRef, useState } from "react";
import Link from "next/link";
import { ArrowLeftIcon, BookOpenIcon, DotsThreeIcon, NotePencilIcon, StackIcon } from "@phosphor-icons/react/ssr";
import { Button, DropdownMenu, DropdownMenuContent, DropdownMenuItem, DropdownMenuTrigger, Input, Label, PageTitle } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { CHAT_TITLE_LIMIT, chatTitle, firstChatRequest } from "@/lib/api/draft-summary";
import { DRAFT_STATE } from "@/lib/api/draft-types";

type Props = {client: DraftClient; view: DraftView; workspace: string; fleetPage?: boolean; returnToFleet?: string; menuItems?: ReactNode};

export function DraftChatHeading(props: Props) {
  return props.view.inaccessible ? null : <HeadingContent key={props.view.draft?.id} {...props} />;
}

function HeadingContent({client, view, workspace, fleetPage, returnToFleet, menuItems}: Props) {
  const titleId = useId();
  const [title, setTitle] = useState<string | null>(null);
  const nameInput = useRef<HTMLInputElement>(null);
  const renaming = title !== null;
  useEffect(() => {if (renaming) nameInput.current?.focus();}, [renaming]);
  const draft = view.draft;
  const empty = !draft && !view.preview && !view.fleet;
  const editing = Boolean(draft && draft.state === DRAFT_STATE.editing && !savedDraft(view));
  const locked = view.busy || view.pending || chatRecovery(view).pending;
  const label = view.fleet?.name ?? draft?.review?.effective.instance_name ?? view.preview?.instanceName
    ?? (draft ? chatTitle(draft.title, draft.id, firstChatRequest(view.messages) || draft.candidate?.definition.description) : "New fleet");
  const returnFleet = fleetPage ? undefined : view.fleet?.id ?? returnToFleet;
  const Title = empty || fleetPage ? "h2" : "h1";
  const Icon = empty ? NotePencilIcon : StackIcon;
  const rename = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (title?.trim() && !locked) void client.rename(title.trim()).then(() => setTitle(null)).catch(() => undefined);
  };

  return <header className="fleet-chat-heading flex min-h-16 w-full shrink-0 items-center gap-lg border-b border-border-subtle px-lg summary-medium:px-xl summary-wide:px-2xl">
    {title !== null && editing ? <>
      <Title className="sr-only">{label}</Title>
      <form className="flex min-w-0 flex-1 items-center gap-sm" onSubmit={rename}>
        <Label htmlFor={titleId} className="sr-only">Chat name</Label>
        <Input id={titleId} ref={nameInput} value={title} onChange={event => setTitle(event.target.value)} maxLength={CHAT_TITLE_LIMIT} className="min-w-0 flex-1" />
        <Button type="submit" size="sm" disabled={locked || !title.trim()}>Save name</Button>
        <Button type="button" variant="ghost" size="sm" disabled={locked} onClick={() => setTitle(null)}>Cancel</Button>
      </form>
    </> : <div className="flex min-w-0 flex-1 items-center gap-lg">
      <Icon data-app-header-icon="" size={16} className="hidden shrink-0 text-muted-foreground sm:block" aria-hidden="true" />
      <PageTitle as={Title} data-app-header-title="" size="chat" className="truncate font-semibold">{label}</PageTitle>
      {editing || view.preview && !fleetPage && !view.savedSetupRevision ? <span className="inline-flex shrink-0 items-center gap-sm text-eyebrow text-muted-foreground"><span aria-hidden="true" className="size-1.5 rounded-full bg-current" />Draft</span> : null}
      {draft && editing && !view.fleet ? <DropdownMenu><DropdownMenuTrigger asChild>
        <Button variant="ghost" size="icon-compact" aria-label="Draft actions"><DotsThreeIcon size={16} aria-hidden="true" /></Button>
      </DropdownMenuTrigger><DropdownMenuContent align="start">
        <DropdownMenuItem disabled={locked} onSelect={() => setTitle(draft.title)}>Rename chat</DropdownMenuItem>
      </DropdownMenuContent></DropdownMenu> : null}
    </div>}
    {returnFleet ? <Button asChild variant="ghost" size="icon" aria-label="Back to fleet conversation"><Link href={`/w/${workspace}/fleets/${returnFleet}`}><ArrowLeftIcon size={16} aria-hidden="true" /></Link></Button> : null}
    {empty ? <Button variant="ghost" size="icon-header" aria-label="New fleet" disabled={locked} onClick={() => {setTitle(null);client.fresh();}}><NotePencilIcon size={16} aria-hidden="true" /></Button> : null}
    {menuItems ? <DropdownMenu><DropdownMenuTrigger asChild><Button variant="ghost" size="sm"><BookOpenIcon size={16} aria-hidden="true" />Memory</Button></DropdownMenuTrigger><DropdownMenuContent align="end">{menuItems}</DropdownMenuContent></DropdownMenu> : null}
  </header>;
}
