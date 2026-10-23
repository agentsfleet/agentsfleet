"use client";

import { ComposerPrimitive } from "@assistant-ui/react";
import { useRef } from "react";
import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { Button } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { CHAT_PLACEHOLDER } from "./chat-defaults";
import { ChatToolbar } from "./ChatToolbar";
import { ChatComposerFrame } from "./ChatComposerFrame";

// DraftClient records this failure in view.error before rejecting the request.
const errorAlreadyVisible = (): void => undefined;

export function ChatComposer({ client, view, canStop, blocked }: { client: DraftClient; view: DraftView; canStop: boolean; blocked: boolean }) {
  const stopDisabled = view.busy || view.pending || view.inaccessible || Boolean(view.draft && view.draft.state !== DRAFT_STATE.editing);
  const disabled = stopDisabled || !view.model || blocked;
  const input = useRef<HTMLTextAreaElement>(null);
  return <>
    <ChatComposerFrame label="Private chat composer" placeholder={CHAT_PLACEHOLDER} inputRef={input} disabled={disabled} focusKey={view.focusRevision}
      toolbar={<ChatToolbar client={client} view={view} />}
      actions={canStop ? <Button variant="secondary" size="icon" aria-label="Stop authoring" disabled={stopDisabled} onClick={() => { void client.stop().catch(errorAlreadyVisible); }}><SquareIcon className="size-4" /></Button>
        : <ComposerPrimitive.Send asChild><Button size="icon" aria-label="Send message" onClick={() => input.current?.focus({preventScroll: true})} {...(disabled ? {disabled: true} : {})}><ArrowUpIcon className="size-4" /></Button></ComposerPrimitive.Send>} />
    <p className="mt-sm text-center text-eyebrow text-muted-foreground">{view.preview?.setupRevision || view.draft?.linked_fleet_id
      ? "Private to you. Save changes when you’re ready." : "Private to you. Save fleet when the setup is ready."}</p>
  </>;
}
