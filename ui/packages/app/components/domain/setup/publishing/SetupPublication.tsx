"use client";

import { useRef, useState } from "react";
import { CaretRightIcon } from "@phosphor-icons/react/ssr";
import Link from "next/link";
import { Button } from "@agentsfleet/design-system";
import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { DRAFT_STATE } from "@/lib/api/draft-types";
import { savedDraft } from "@/lib/api/draft-continuation";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { useAuthoringIntent } from "../AuthoringIntent";
import { SetupStatusFlap } from "./SetupStatusFlap";
import { ChatProgressLabel } from "../../shared/chat/ChatProgressLabel";

type Props = {client: DraftClient; view: DraftView; workspace: string; dirty: boolean; onOpenFleet?: () => void; modelsHref?: string};

export function SetupPublication({client, view, dirty, ...statusProps}: Props) {
  const {publicationTarget} = useAuthoringIntent();
  const inFlight = useRef(false);
  const [preparing, setPreparing] = useState(false);
  const published = view.draft?.publication?.canonical_digest === view.draft?.candidate?.canonical_digest
    && Boolean(view.draft?.publication);
  const editable = view.draft ? !savedDraft(view) && view.draft.state === DRAFT_STATE.editing && Boolean(view.draft.candidate && view.draft.review)
    : Boolean(view.preview);
  const label = view.draft?.review?.effective.instance_name ?? view.preview?.instanceName ?? "";
  const available = editable && !published && !view.inaccessible && !view.busy && !view.pending && !view.error
    && Boolean(label.trim()) && !dirty && !chatRecovery(view).pending;
  const publish = async (): Promise<void> => {
    if (inFlight.current) return;
    inFlight.current = true; setPreparing(true);
    try {
      const source = client.getSnapshot();
      if (source.preview && !source.draft) {
        await client.prepareSave();
        await client.refreshSetup();
      }
      const current = client.getSnapshot();
      const target = publicationTarget ?? current.draft?.publication?.plugin_id;
      await client.publish(label, Boolean(target), target);
    } catch { /* DraftClient retains the refused or unconfirmed operation for recovery. */ }
    finally {inFlight.current = false; setPreparing(false);}
  };
  if (!view.inaccessible && !view.error && (preparing || view.busy && view.setupAction === "publishing")) return <div className="mb-sm flex justify-center">
    <output aria-label="Publishing setup" aria-live="polite" aria-busy="true" data-setup-status="working"
      className="inline-flex max-w-full items-center rounded-full border border-border bg-card px-lg py-sm text-label text-muted-foreground">
      <ChatProgressLabel label="Saving and publishing" />
    </output>
  </div>;
  return available ? <div className="mb-sm flex flex-wrap items-center justify-center gap-md">
    <Button variant="outline" className="rounded-full bg-card px-xl" onClick={() => {void publish();}}>Save and publish<CaretRightIcon size={16} className="text-muted-foreground" aria-hidden="true" /></Button>
    {view.fleet ? statusProps.onOpenFleet ? <Button variant="link" size="eyebrow" onClick={statusProps.onOpenFleet}>Open fleet</Button>
      : <Link className="text-eyebrow text-muted-foreground hover:underline" href={`/w/${statusProps.workspace}/fleets/${view.fleet.id}`}>Open fleet</Link> : null}
  </div> : <SetupStatusFlap view={view} dirty={dirty} showPublication {...statusProps} />;
}
