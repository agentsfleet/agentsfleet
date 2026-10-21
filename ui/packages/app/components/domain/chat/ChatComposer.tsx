"use client";

import { ComposerPrimitive } from "@assistant-ui/react";
import { useEffect, useRef } from "react";
import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { Button, DashboardPanel, Textarea } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { CHAT_PLACEHOLDER } from "./chat-defaults";
import { ChatToolbar } from "./ChatToolbar";

// DraftClient records this failure in view.error before rejecting the request.
const errorAlreadyVisible = (): void => undefined;

export function ChatComposer({ client, view, canStop, blocked }: { client: DraftClient; view: DraftView; canStop: boolean; blocked: boolean }) {
  const stopDisabled = view.busy || view.pending || view.inaccessible || Boolean(view.draft && view.draft.state !== DRAFT_STATE.editing);
  const disabled = stopDisabled || !view.model || blocked;
  const input = useRef<HTMLTextAreaElement>(null);
  const fresh = !view.draft && !view.fleet && !view.preview;
  // Fresh replaces messages even when the person clicks New fleet twice.
  useEffect(() => {
    if (fresh && !disabled) input.current?.focus({preventScroll: true});
  }, [fresh, disabled, view.messages]);
  return <>
    <DashboardPanel padding="compact" className="focus-surface rounded-xl">
      <ComposerPrimitive.Root className="flex flex-col gap-md" aria-label="Private chat composer">
        <ComposerPrimitive.Input ref={input} asChild placeholder={CHAT_PLACEHOLDER} submitMode="enter" disabled={disabled}>
          <Textarea rows={2} aria-label={CHAT_PLACEHOLDER} className="max-h-48 min-h-20 resize-none border-0 bg-transparent shadow-none" />
        </ComposerPrimitive.Input>
        <div className="flex items-end gap-sm">
          <ChatToolbar client={client} view={view} />
          {canStop ? <Button variant="secondary" size="icon" aria-label="Stop authoring" disabled={stopDisabled} onClick={() => { void client.stop().catch(errorAlreadyVisible); }}><SquareIcon className="size-4" /></Button>
            : <ComposerPrimitive.Send asChild><Button size="icon" aria-label="Send message" disabled={disabled}><ArrowUpIcon className="size-4" /></Button></ComposerPrimitive.Send>}
        </div>
      </ComposerPrimitive.Root>
    </DashboardPanel>
    <p className="mt-md text-center text-label text-muted-foreground">Private to you. Save when you’re ready to create your fleet.</p>
  </>;
}
