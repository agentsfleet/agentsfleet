"use client";

import type { DraftClient } from "@/lib/api/drafts";
import type { DraftView } from "@/lib/api/draft-state";
import { useState } from "react";
import { Button, Section } from "@agentsfleet/design-system";
import { DRAFT_STATE, isRunning } from "@/lib/api/draft-types";
import { SetupProgress } from "./SetupProgress";
import { DefinitionEditor } from "./DefinitionEditor";
import { FleetSetupDetails } from "./FleetSetupDetails";
import { FleetSetupActions } from "./FleetSetupActions";

export function FleetSetup({client, view, workspace}: {client: DraftClient; view: DraftView; workspace: string}) {
  const [editing, setEditing] = useState(false);
  if (view.fleet) return <Section className="my-lg gap-md">
    <h2 className="text-body font-semibold">Fleet saved</h2>
    <SetupProgress fleet={view.fleet} definition={view.fleet.definition ?? view.draft?.candidate?.definition} workspace={workspace} busy={view.busy}
      onContinue={() => {void client.retry().catch(() => undefined);}} onRetrySchedule={(schedule) => {void client.syncSchedule(schedule).catch(() => undefined);}} />
  </Section>;
  if (view.inaccessible || (!view.draft?.candidate && !view.preview)) return null;
  const setup = view.draft?.review;
  const disabled = view.busy || view.pending || view.messages.some(isRunning) || Boolean(view.draft && view.draft.state !== DRAFT_STATE.editing);
  return <Section className="ph-no-capture ph-no-record my-lg gap-lg rounded-xl border border-border p-lg" aria-label="Fleet setup">
    <div className="flex items-center justify-between gap-md">
      <h2 className="text-body font-semibold">{setup?.effective.target_setup_revision ? "Update fleet setup" : "Your fleet setup"}</h2>
      <Button variant="ghost" size="sm" disabled={disabled} onClick={() => setEditing(!editing)}>{editing ? "Cancel editing" : "Edit instructions"}</Button>
    </div>
    <p className="text-label text-muted-foreground">Keep chatting to change what your fleet does. Save when the instructions are ready.</p>
    {editing || !view.draft ? <DefinitionEditor key={view.draft?.candidate?.id ?? view.preview?.revision} client={client} view={view} onUpdated={() => setEditing(false)} />
      : setup ? <><FleetSetupDetails review={setup} /><FleetSetupActions client={client} view={view} setup={setup} /></>
      : <p className="text-label text-muted-foreground">Continue chatting to finish these instructions before saving.</p>}
  </Section>;
}
