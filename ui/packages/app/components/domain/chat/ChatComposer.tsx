"use client";

import { ComposerPrimitive } from "@assistant-ui/react";
import { ArrowUpIcon, SquareIcon } from "lucide-react";
import { Button, DashboardPanel, Textarea } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { CHAT_PLACEHOLDER } from "./chat-defaults";
import { ChatToolbar } from "./ChatToolbar";

export function ChatComposer({ client, view, running, onReview }: { client: DraftClient; view: DraftView; running: boolean; onReview: () => void }) {
  const disabled = view.busy || view.pending || view.inaccessible || !view.model || Boolean(view.draft && view.draft.state !== DRAFT_STATE.editing);
  return <>
    <DashboardPanel padding="compact" className="chat-composer rounded-xl">
      <ComposerPrimitive.Root className="flex flex-col gap-md" aria-label="Private chat composer">
        <ComposerPrimitive.Input asChild placeholder={CHAT_PLACEHOLDER} submitMode="enter" disabled={disabled}>
          <Textarea rows={2} aria-label={CHAT_PLACEHOLDER} className="max-h-48 min-h-20 resize-none border-0 bg-transparent shadow-none" />
        </ComposerPrimitive.Input>
        <div className="flex items-end gap-sm">
          <ChatToolbar client={client} view={view} onReview={onReview} />
          {running ? <ComposerPrimitive.Cancel asChild><Button variant="secondary" size="icon" aria-label="Stop authoring"><SquareIcon className="size-4" /></Button></ComposerPrimitive.Cancel>
            : <ComposerPrimitive.Send asChild><Button size="icon" aria-label="Send message" disabled={disabled}><ArrowUpIcon className="size-4" /></Button></ComposerPrimitive.Send>}
        </div>
      </ComposerPrimitive.Root>
    </DashboardPanel>
    <p className="mt-md text-center text-label text-muted-foreground">Private to you. You approve before your fleet runs.</p>
  </>;
}
