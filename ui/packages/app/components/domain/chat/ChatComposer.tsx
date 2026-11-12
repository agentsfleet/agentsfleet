"use client";

import { ComposerPrimitive } from "@assistant-ui/react";
import { useRef } from "react";
import { ArrowUpIcon, NotePencilIcon, SquareIcon } from "@phosphor-icons/react/ssr";
import { Button, cn } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { savedDraft } from "@/lib/api/draft-continuation";
import { CHAT_PLACEHOLDER } from "./chat-defaults";
import { ChatToolbar } from "./ChatToolbar";
import { ChatComposerFrame, ChatComposerHint } from "./ChatComposerFrame";
import { SetupStatusFlap } from "./SetupStatusFlap";

// DraftClient records this failure in view.error before rejecting the request.
const errorAlreadyVisible = (): void => undefined;

export function ChatComposer({ client, view, workspace, canStop, blocked, dirty, onOpenFleet, modelsHref, empty = false }: { empty?: boolean; client: DraftClient; view: DraftView; workspace: string; canStop: boolean; blocked: boolean; dirty: boolean; onOpenFleet?: () => void; modelsHref?: string }) {
  const stopDisabled = view.busy || view.pending || view.inaccessible || Boolean(view.draft && view.draft.state !== DRAFT_STATE.editing && !savedDraft(view));
  const disabled = stopDisabled || !view.model || blocked;
  const input = useRef<HTMLTextAreaElement>(null);
  return <>
    <SetupStatusFlap view={view} workspace={workspace} dirty={dirty} onOpenFleet={onOpenFleet} modelsHref={modelsHref} />
    <ChatComposerFrame label="Setup chat composer" placeholder={view.fleet || view.preview?.setupRevision ? "Ask a question or change the setup…" : CHAT_PLACEHOLDER} inputRef={input} disabled={disabled} focusKey={view.focusRevision}
      toolbar={<ChatToolbar client={client} view={view} />}
      actions={canStop ? <Button variant="secondary" size="icon" className="size-9 [&_svg]:size-4.5" aria-label="Stop authoring" disabled={stopDisabled} onClick={() => { void client.stop().catch(errorAlreadyVisible); }}><SquareIcon className="size-4.5" /></Button>
        : <ComposerPrimitive.Send asChild><Button size={empty ? "default" : "icon"} className={cn("disabled:border-transparent disabled:bg-muted disabled:text-muted-foreground disabled:opacity-100 [&_svg]:size-4.5", !empty && "size-9 rounded-full")} aria-label={empty ? "New fleet from message" : "Send message"} onClick={() => input.current?.focus({preventScroll: true})} {...(disabled ? {disabled: true} : {})}>{empty ? <><NotePencilIcon size={16} />New fleet</> : <ArrowUpIcon className="size-4.5" />}</Button></ComposerPrimitive.Send>} />
    <ChatComposerHint>{empty ? null : view.preview?.setupRevision || view.draft?.linked_fleet_id
      ? "Save changes when you’re ready." : "Save fleet when the setup is ready."}</ChatComposerHint>
  </>;
}
