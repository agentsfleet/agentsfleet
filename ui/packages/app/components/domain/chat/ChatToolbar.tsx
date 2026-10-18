"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@agentsfleet/design-system";
import { isRunning } from "@/lib/api/draft-types";
import { DRAFT_LIST } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "./DraftLoadMore";

const ignore = (): void => undefined;

export function ChatToolbar({ client, view }: { client: DraftClient; view: DraftView }) {
  "use memo";
  const locked = view.busy || view.pending || view.inaccessible || view.messages.some(isRunning);
  const models = view.models.filter((model) => model.eligibility.authoring);
  return <div className="flex min-w-0 flex-1 flex-wrap items-center gap-sm">
    <Select value={view.model} disabled={locked || models.length === 0} onValueChange={(model) => { void client.selectModel(model).catch(ignore); }}>
      <SelectTrigger aria-label="Authoring model" className="w-auto min-w-0 max-w-full border-0 bg-transparent"><SelectValue placeholder="Choose model" /></SelectTrigger>
      <SelectContent>{models.map((model) => <SelectItem key={model.ref} value={model.ref}>{model.label}</SelectItem>)}</SelectContent>
    </Select>
    {view.moreModels && !view.inaccessible ? <DraftLoadMore copy={LOAD_MORE_COPY.models} onLoad={() => client.loadMore(DRAFT_LIST.models)} /> : null}
  </div>;
}
