"use client";

import { ComposerPrimitive } from "@assistant-ui/react";
import { useRef } from "react";
import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { Button } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { savedDraft } from "@/lib/api/draft-continuation";
import { CHAT_PLACEHOLDER } from "./chat-defaults";
import { ChatToolbar } from "./ChatToolbar";
import { ChatComposerFrame, ChatComposerHint } from "./ChatComposerFrame";

// DraftClient records this failure in view.error before rejecting the request.
const errorAlreadyVisible = (): void => undefined;

export function ChatComposer({ client, view, canStop, blocked }: { client: DraftClient; view: DraftView; canStop: boolean; blocked: boolean }) {
  const stopDisabled = view.busy || view.pending || view.inaccessible || Boolean(view.draft && view.draft.state !== DRAFT_STATE.editing && !savedDraft(view));
  const disabled = stopDisabled || !view.model || blocked;
  const input = useRef<HTMLTextAreaElement>(null);
  return <>
    <ChatComposerFrame label="Setup chat composer" placeholder={view.fleet || view.preview?.setupRevision ? "Ask a question or change the setup…" : CHAT_PLACEHOLDER} inputRef={input} disabled={disabled} focusKey={view.focusRevision}
      toolbar={<ChatToolbar client={client} view={view} />}
      actions={canStop ? <Button variant="secondary" size="icon" aria-label="Stop authoring" disabled={stopDisabled} onClick={() => { void client.stop().catch(errorAlreadyVisible); }}><SquareIcon className="size-4" /></Button>
        : <ComposerPrimitive.Send asChild><Button size="icon" aria-label="Send message" onClick={() => input.current?.focus({preventScroll: true})} {...(disabled ? {disabled: true} : {})}><ArrowUpIcon className="size-4" /></Button></ComposerPrimitive.Send>} />
    <ChatComposerHint>{view.preview?.setupRevision || view.draft?.linked_fleet_id
      ? "Save changes when you’re ready." : "Save fleet when the setup is ready."}</ChatComposerHint>
  </>;
}
