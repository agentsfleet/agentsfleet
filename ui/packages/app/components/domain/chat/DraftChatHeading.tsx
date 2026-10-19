"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useState } from "react";
import { Button, Input, Label } from "@agentsfleet/design-system";
import { PencilIcon } from "lucide-react";
import { DRAFT_STATE, isRunning } from "@/lib/api/draft-types";
import { CHAT_TITLE_LIMIT, chatTitle, firstChatRequest } from "@/lib/api/draft-summary";

export function DraftChatHeading({client, view}: {client: DraftClient; view: DraftView}) {
  const [editing, setEditing] = useState(false);
  const [title, setTitle] = useState("");
  const draft = view.draft;
  if (!draft) return null;
  const locked = view.busy || view.pending || view.inaccessible || view.messages.some(isRunning) || draft.state !== DRAFT_STATE.editing;
  const label = chatTitle(draft.title, draft.id, firstChatRequest(view.messages) || draft.candidate?.definition.description);
  return <div className="mx-auto flex w-full max-w-measure flex-wrap items-center gap-sm px-sm">
    {editing ? <><Label htmlFor={`${draft.id}-title`} className="sr-only">Chat name</Label>
      <Input id={`${draft.id}-title`} value={title} onChange={(event) => setTitle(event.target.value)} maxLength={CHAT_TITLE_LIMIT} className="min-w-0 flex-1" />
      <Button size="sm" disabled={locked || !title.trim()} onClick={() => {void client.rename(title.trim()).then(() => setEditing(false)).catch(() => undefined);}}>Save name</Button>
      <Button variant="ghost" size="sm" disabled={locked} onClick={() => setEditing(false)}>Cancel</Button></>
      : <><h2 className="min-w-0 flex-1 truncate text-body font-semibold">{label}</h2>
        <Button variant="ghost" size="sm" aria-label="Rename setup chat" disabled={locked}
          onClick={() => {setTitle(label); setEditing(true);}}><PencilIcon size={14} aria-hidden="true" />Rename</Button></>}
  </div>;
}
