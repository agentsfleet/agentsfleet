"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import type { SetupNetworkChange } from "@/lib/setup-requirements";
import { useState } from "react";
import nextDynamic from "next/dynamic";
import { z } from "zod";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@agentsfleet/design-system";
import { savedDraft } from "@/lib/api/draft-continuation";
import { runtimeModelLabel } from "@/lib/api/draft-list-types";
import { chatRecovery } from "@/lib/api/draft-recovery";
import { SCHEDULE_SYNC } from "@/lib/api/draft-vocabulary";
import { setupRequirements } from "@/lib/setup-requirements";
import {pluginsUiEnabled} from "@/lib/plugin-ui";
import { SetupSummary } from "./SetupSummary";
import { ScheduleSync } from "../../schedules/ScheduleSync";

const ExtensionSchema = z.record(z.string(), z.unknown());
const SelectionEditor = nextDynamic(() => import("../editor/SetupSelectionEditor").then(module => module.SetupSelectionEditor));
type Selection = "repositories" | "credentials";
type Change = {repositories: readonly string[]} | {credentials: readonly string[]} | {network: Record<string, unknown>};

/** Summary reads confirmed state; edits stay in the creator-owned draft. */
export function SetupReview({view, client, workspace}: {view: DraftView; client: DraftClient; workspace: string}) {
  const [selection, setSelection] = useState<Selection | null>(null);
  const proposed = Boolean(view.draft?.candidate && !savedDraft(view));
  const review = proposed ? view.draft?.review : undefined;
  const preview = !view.fleet || view.preview?.setupRevision === view.fleet.setup_revision ? view.preview : null;
  const definition = proposed ? view.draft?.candidate?.definition : view.fleet?.definition ?? preview?.definition;
  const name = review?.effective.instance_name ?? view.fleet?.name ?? preview?.instanceName;
  const extension = ExtensionSchema.safeParse(definition?.config["x-agentsfleet"]);
  const network = ExtensionSchema.safeParse(extension.success ? extension.data.network ?? {} : undefined);
  const runtime = review ? review.effective.runtime_model_ref : extension.success && typeof extension.data.model === "string" ? extension.data.model : null;
  const modelLabel = runtime || (view.fleet && !proposed && definition)
    ? runtimeModelLabel(view.models, runtime) : view.fleet && !proposed ? "Unavailable" : "Not resolved";
  const requirements = setupRequirements(definition, review?.requirements);
  const locked = view.busy || view.pending || view.inaccessible || chatRecovery(view).pending;
  const schedules = view.fleet?.schedules ?? view.schedules;
  const unsynced = schedules.filter(schedule => schedule.sync !== SCHEDULE_SYNC.synced && schedule.status !== "removed" && schedule.status !== "expired");
  const apply = definition && name && extension.success ? async (change: Change): Promise<void> => {
    const values = "repositories" in change ? repositorySelection(extension.data, change.repositories)
      : {...extension.data, ...change};
    await client.edit({definition: {...definition, config: {...definition.config, "x-agentsfleet": values}}, instanceName: name});
    await client.refreshSetup();
  } : undefined;
  const editable = Boolean(apply && requirements && !view.inaccessible);
  return <>
    <SetupSummary definition={definition} requirements={review?.requirements} modelLabel={modelLabel} sourceLabel={pluginsUiEnabled() ? preview?.sourceLabel : undefined}
      ready={Boolean(view.fleet)} disabled={locked}
      scheduleRecovery={unsynced.length ? <ScheduleSync compact schedules={unsynced} busy={locked} onRetry={schedule => {void client.syncSchedule(schedule).catch(() => undefined);}} /> : undefined}
      onManage={editable ? setSelection : undefined}
      onNetworkChange={editable && apply && network.success ? (change: SetupNetworkChange) => {void apply({network: {...network.data, ...change}}).catch(() => undefined);} : undefined} />
    <Dialog open={selection !== null && editable} onOpenChange={() => {if (!view.busy) setSelection(null);}}>
      <DialogContent className="ph-no-capture ph-no-record"><DialogHeader><DialogTitle>{selection === "repositories" ? "Repositories" : "Secrets"}</DialogTitle>
        <DialogDescription>Choose the access this fleet needs. Changes remain in its setup until saved.</DialogDescription></DialogHeader>
        {selection && requirements && apply ? <SelectionEditor key={selection} kind={selection} workspace={workspace} selected={requirements[selection]}
          disabled={locked} pending={view.pending} onRetry={async () => {await client.retry();await client.refreshSetup();setSelection(null);}} onReload={() => client.refreshSetup().then(() => undefined)}
          onCancel={() => setSelection(null)} onApply={async names => {await apply(selection === "repositories" ? {repositories: names} : {credentials: names});setSelection(null);}} /> : null}
      </DialogContent>
    </Dialog>
  </>;
}

/** A repository binding is optional as a whole; new bindings start read-only. */
function repositorySelection(extension: Record<string, unknown>, repositories: readonly string[]) {
  const values = {...extension};
  if (repositories.length) return {...values, repositories, repository_access: values.repository_access ?? "read"};
  for (const key of ["repositories", "repository_access", "repository_base", "repository_publication", "repository_validation_workflows"]) delete values[key];
  return values;
}
