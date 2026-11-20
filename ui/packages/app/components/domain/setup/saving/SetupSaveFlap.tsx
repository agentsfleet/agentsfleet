"use client";

import {useRef, useState} from "react";
import {CaretRightIcon} from "@phosphor-icons/react/ssr";
import {Button} from "@agentsfleet/design-system";
import type {DraftClient} from "@/lib/api/drafts";
import type {DraftView} from "@/lib/api/draft-state";
import {DRAFT_STATE} from "@/lib/api/draft-types";
import {savedDraft} from "@/lib/api/draft-continuation";
import {chatRecovery} from "@/lib/api/draft-recovery";
import {SetupStatusFlap} from "./SetupStatusFlap";
import {ChatProgressLabel} from "../../shared/chat/ChatProgressLabel";

type Props = {client: DraftClient; view: DraftView; workspace: string; dirty: boolean; onOpenFleet?: () => void; modelsHref?: string};

/** Saves the reviewed setup as a new fleet, or as the next revision of the fleet it edits. */
export function SetupSaveFlap({client, view, dirty, ...statusProps}: Props) {
  const inFlight = useRef(false);
  const [preparing, setPreparing] = useState(false);
  const editable = view.draft ? !savedDraft(view) && view.draft.state === DRAFT_STATE.editing && Boolean(view.draft.candidate && view.draft.review)
    : Boolean(view.preview);
  const name = view.draft?.review?.effective.instance_name ?? view.preview?.instanceName;
  const available = editable && Boolean(name?.trim()) && !view.inaccessible && !view.busy && !view.pending && !view.error && !dirty && !chatRecovery(view).pending;
  const save = async (): Promise<void> => {
    if (inFlight.current) return;
    inFlight.current = true;setPreparing(true);
    try {
      const source = client.getSnapshot();
      if (source.preview && !source.draft) {
        await client.prepareSave();
        await client.refreshSetup();
      }
      const current = client.getSnapshot();
      if (current.draft?.linked_fleet_id || current.preview?.setupRevision) await client.apply();
      else await client.save();
    } catch { /* Native recovery retains refused or unconfirmed saves and their operation identity. */ }
    finally {inFlight.current = false;setPreparing(false);}
  };
  if (!view.inaccessible && !view.error && (preparing || view.busy && view.setupAction === "saving")) return <div className="mb-sm flex justify-center">
    <output aria-label="Saving setup" aria-live="polite" aria-busy="true" data-setup-status="working"
      className="inline-flex max-w-full items-center rounded-full border border-border bg-card px-lg py-sm text-label text-muted-foreground">
      <ChatProgressLabel label="Saving" />
    </output>
  </div>;
  return available ? <div className="mb-sm flex justify-center"><Button variant="outline" className="rounded-full bg-card px-xl" onClick={() => {void save();}}>
    Save<CaretRightIcon size={16} className="text-muted-foreground" aria-hidden="true" />
  </Button></div> : <SetupStatusFlap view={view} dirty={dirty} {...statusProps} />;
}
