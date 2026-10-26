"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Select, SelectContent, SelectItem, SelectValue } from "@agentsfleet/design-system";
import { isRunning } from "@/lib/api/draft-types";
import { DRAFT_LIST } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "./DraftLoadMore";
import { ChatComposerModel } from "./ChatComposerFrame";

const ignore = (): void => undefined;

export function ChatToolbar({ client, view }: { client: DraftClient; view: DraftView }) {
  "use memo";
  const locked = view.busy || view.pending || view.inaccessible || view.messages.some(isRunning);
  const models = view.models.filter((model) => model.eligibility.authoring);
  return <div className="flex min-w-0 flex-1 flex-wrap items-center gap-sm">
    <Select value={view.model} disabled={locked || models.length === 0} onValueChange={(model) => { void client.selectModel(model).catch(ignore); }}>
      <ChatComposerModel select aria-label="Authoring model" title="Model for this setup conversation"><SelectValue placeholder="Choose model" /></ChatComposerModel>
      <SelectContent>{models.map((model) => <SelectItem key={model.ref} value={model.ref}>{model.label}</SelectItem>)}</SelectContent>
    </Select>
    {view.moreModels && !view.inaccessible ? <DraftLoadMore copy={LOAD_MORE_COPY.models} onLoad={() => client.loadMore(DRAFT_LIST.models)} /> : null}
  </div>;
}
