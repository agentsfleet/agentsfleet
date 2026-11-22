"use client";

import type { RefObject } from "react";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { Button, Select, SelectContent, SelectItem, SelectValue, cn } from "@agentsfleet/design-system";
import { isRunning } from "@/lib/api/draft-types";
import { DRAFT_LIST, DRAFT_LIST_STATUS, MODEL_LIST_RETRYING } from "@/lib/api/draft-list-types";
import { DraftLoadMore, LOAD_MORE_COPY } from "../conversation/DraftLoadMore";
import { ChatComposerModel } from "../../shared/chat/ChatComposerFrame";

const ignore = (): void => undefined;

export function ChatToolbar({ client, view, inputRef }: { client: DraftClient; view: DraftView; inputRef: RefObject<HTMLTextAreaElement | null> }) {
  "use memo";
  const locked = view.busy || view.pending || view.inaccessible || view.messages.some(isRunning);
  const models = view.models.filter((model) => model.eligibility.setup);
  const hasChoices = models.length > 0;
  const retrying = view.modelListStatus === MODEL_LIST_RETRYING;
  const retry = (): void => {
    inputRef.current?.focus({preventScroll: true});
    void client.retryModels().catch(ignore);
  };
  return <div className="flex min-w-0 flex-wrap items-center gap-sm">
    <Select value={hasChoices ? view.model : ""} disabled={locked || !hasChoices} onValueChange={(model) => { void client.selectModel(model).catch(ignore); }}>
      <ChatComposerModel select aria-label="Setup model" title="Model for this setup conversation"><SelectValue placeholder="Choose model" /></ChatComposerModel>
      <SelectContent>{models.map((model) => <SelectItem key={model.ref} value={model.ref}>{model.label}</SelectItem>)}</SelectContent>
    </Select>
    {retrying || view.modelListStatus === DRAFT_LIST_STATUS.unavailable ? <>
      <output className={cn("text-eyebrow leading-eyebrow", retrying ? "text-muted-foreground" : "text-destructive")}>{retrying ? "Loading models…" : "Couldn’t load models"}</output>
      <Button type="button" variant="link" size="eyebrow" className="text-eyebrow leading-eyebrow" aria-label="Retry models" aria-disabled={retrying} onClick={retry}>Retry</Button>
    </> : null}
    {view.moreModels && !view.inaccessible ? <DraftLoadMore copy={LOAD_MORE_COPY.models} onLoad={() => client.loadMore(DRAFT_LIST.models)} /> : null}
  </div>;
}
