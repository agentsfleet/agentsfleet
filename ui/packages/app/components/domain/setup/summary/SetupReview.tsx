"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { SetupNetworkChange } from "@/lib/setup-requirements";
import { useRef, useState } from "react";
import nextDynamic from "next/dynamic";
import { z } from "zod";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { runtimeModelLabel } from "@/lib/api/draft-list-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { setupRequirements } from "@/lib/setup-requirements";
import { SetupSummary } from "./SetupSummary";
import { ScheduleSync } from "../../schedules/ScheduleSync";

const ExtensionSchema = z.record(z.string(), z.unknown());
const SelectionEditor = nextDynamic(() => import("../editor/SetupSelectionEditor").then(module => module.SetupSelectionEditor));
type Selection = "repositories" | "credentials";
type Change = {repositories: readonly string[]} | {credentials: readonly string[]} | {network: Record<string, unknown>};

/** Summary reads confirmed state; edits stay in the creator-owned draft. */
export function SetupReview({view, client, workspace}: {view: DraftView; client: DraftClient; workspace: string}) {
  const [selection, setSelection] = useState<Selection | null>(initialSelection);
  const secretTrigger = useRef<HTMLButtonElement | null>(null);
  const proposed = Boolean(view.draft?.candidate && !savedDraft(view));
  const review = proposed ? view.draft?.review : undefined;
  const preview = !view.fleet || view.preview?.setupRevision === view.fleet.setup_revision ? view.preview : null;
  const definition = proposed ? view.draft?.candidate?.definition : view.fleet?.definition ?? preview?.definition;
  const extension = ExtensionSchema.safeParse(definition?.config["x-agentsfleet"]);
  const network = ExtensionSchema.safeParse(extension.success ? extension.data.network ?? {} : undefined);
  const runtime = review ? review.effective.runtime_model_ref : extension.success && typeof extension.data.model === "string" ? extension.data.model : null;
  const modelLabel = runtime || (view.fleet && !proposed && definition)
    ? runtimeModelLabel(view.models, runtime) : view.fleet && !proposed ? "Unavailable" : "Not resolved";
  const requirements = setupRequirements(definition, review?.requirements);
  const locked = view.busy || view.pending || view.inaccessible || chatRecovery(view).pending;
  const schedules = view.fleet?.schedules ?? view.schedules;
  const unsynced = schedules.filter(schedule => schedule.sync !== SCHEDULE_SYNC.synced && schedule.status !== "removed" && schedule.status !== "expired");
  const apply = definition && extension.success ? (change: Change): Promise<void> => {
    const values = "repositories" in change ? repositorySelection(extension.data, change.repositories)
      : {...extension.data, ...change};
    return client.edit({definition: {...definition, config: {...definition.config, "x-agentsfleet": values}}});
  } : undefined;
  const editable = Boolean(apply && requirements && !view.inaccessible);
  const closeSelection = () => {
    setSelection(null);
    if (selection === "credentials") secretTrigger.current?.focus();
  };
  const editor = selection && requirements && apply ? <SelectionEditor key={selection} kind={selection} workspace={workspace} selected={requirements[selection]}
    disabled={locked} pending={view.pending} onRetry={async () => {await client.retry();await client.refreshSetup();closeSelection();}} onReload={() => client.refreshSetup().then(() => undefined)}
    onCancel={closeSelection} onApply={async names => {await apply(selection === "repositories" ? {repositories: names} : {credentials: names});closeSelection();}} /> : null;
  return <>
    <SetupSummary definition={definition} requirements={review?.requirements} modelLabel={modelLabel}
      ready={Boolean(view.fleet)} disabled={locked} schedules={schedules}
      scheduleRecovery={unsynced.length ? <ScheduleSync compact schedules={unsynced} busy={locked} onRetry={schedule => {void client.syncSchedule(schedule).catch(() => undefined);}} /> : undefined}
      secretEditor={selection === "credentials" ? editor : undefined}
      onManage={editable ? (kind, trigger) => {
        if (kind === "credentials") secretTrigger.current = trigger;
        setSelection(current => current === kind ? null : kind);
      } : undefined}
      onNetworkChange={editable && apply && network.success ? (change: SetupNetworkChange) => {void apply({network: {...network.data, ...change}}).catch(() => undefined);} : undefined} />
    <Dialog open={selection === "repositories" && editable} onOpenChange={() => {if (!view.busy) closeSelection();}}>
      <DialogContent className="ph-no-capture ph-no-record"><DialogHeader><DialogTitle>Repositories</DialogTitle>
        <DialogDescription>Choose the access this fleet needs. Changes remain in its setup until saved.</DialogDescription></DialogHeader>
        {selection === "repositories" ? editor : null}
      </DialogContent>
    </Dialog>
  </>;
}

function initialSelection(): Selection | null {
  if (typeof window === "undefined") return null;
  if (window.location.hash === "#secrets") return "credentials";
  return window.location.hash === "#repositories" ? "repositories" : null;
}

/** A repository binding is optional as a whole; new bindings start read-only. */
function repositorySelection(extension: Record<string, unknown>, repositories: readonly string[]) {
  const values = {...extension};
  if (repositories.length) return {...values, repositories, repository_access: values.repository_access ?? "read"};
  for (const key of ["repositories", "repository_access", "repository_base", "repository_publication", "repository_validation_workflows"]) delete values[key];
  return values;
}
