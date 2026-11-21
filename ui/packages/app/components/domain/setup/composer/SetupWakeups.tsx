"use client";

import nextDynamic from "next/dynamic";
import {ClockIcon} from "@phosphor-icons/react/ssr";
import type {DraftClient} from "@/lib/api/drafts";
import type {DraftView} from "@/lib/api/draft-state";
import {savedDraft} from "@/lib/api/draft-continuation";
import {hasProposedWakeup} from "@/lib/setup-requirements";

const RecordedWakeups = nextDynamic(() => import("../../schedules/RecordedWakeups").then(module => module.RecordedWakeups));
/** Schedule actions stay beside the composer. */
export function SetupWakeups({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  if (view.inaccessible) return null;
  const schedules = view.fleet?.schedules ?? view.schedules;
  if (schedules.length) return <RecordedWakeups key={view.fleet?.id ?? view.draft?.id} workspace={workspace} schedules={schedules}
    onRefresh={() => {void client.refreshSetup().catch(() => undefined);}} />;
  const candidate = !savedDraft(view) ? view.draft?.candidate : undefined;
  return hasProposedWakeup(candidate?.definition)
    ? <output className="mx-5 flex min-h-11 items-center gap-sm rounded-t-md border border-b-0 border-border bg-muted px-lg text-label text-muted-foreground">
      <ClockIcon size={16} aria-hidden="true" />Scheduled wake-up · Pending save and registration
    </output> : null;
}
