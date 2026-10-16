"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Button, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@agentsfleet/design-system";
import { isRunning } from "@/lib/api/draft-types";
import { DRAFT_LIST } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "./DraftLoadMore";

const ignore = (): void => undefined;

export function ChatToolbar({ client, view, onReview }: { client: DraftClient; view: DraftView; onReview: () => void }) {
  "use memo";
  const locked = view.busy || view.pending || view.inaccessible || view.messages.some(isRunning);
  return <div className="flex min-w-0 flex-1 flex-wrap items-center gap-sm">
    <Select value={view.model} disabled={locked} onValueChange={(model) => { void client.selectModel(model).catch(ignore); }}>
      <SelectTrigger aria-label="Authoring model" className="w-auto max-w-full border-0 bg-transparent"><SelectValue placeholder="Choose model" /></SelectTrigger>
      <SelectContent>{view.models.filter((model) => model.eligibility.authoring).map((model) => <SelectItem key={model.ref} value={model.ref}>{model.label}</SelectItem>)}</SelectContent>
    </Select>
    {view.moreModels && !view.inaccessible ? <DraftLoadMore copy={LOAD_MORE_COPY.models} onLoad={() => client.loadMore(DRAFT_LIST.models)} /> : null}
    {view.draft?.candidate || view.preview ? <Button variant="secondary" size="sm" disabled={locked} onClick={onReview}>Review setup</Button> : null}
  </div>;
}
