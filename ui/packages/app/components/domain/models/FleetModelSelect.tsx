"use client";

import { Alert, Select, SelectContent, SelectItem, SelectValue } from "@agentsfleet/design-system";
import { ChatComposerModel } from "../shared/chat/ChatComposerFrame";
import { useFleetModelChoice } from "./FleetModelContext";

const MODEL_LABEL = "Fleet model";
const MODEL_HINT = "The model this fleet's chat, webhook and scheduled runs use";
const EMPTY = "No runtime models available";
const SAVING = "Saving model…";

export function FleetModelSelect({label}: {label: string}) {
  const choice = useFleetModelChoice();
  if (!choice) return <ChatComposerModel>{label}</ChatComposerModel>;
  const models = choice.models.filter(model => model.eligibility.runtime);
  return <div className="flex min-w-0 flex-wrap items-center justify-end gap-sm">
    {choice.changing ? <output className="text-eyebrow text-muted-foreground">{SAVING}</output> : null}
    {choice.error ? <Alert variant="destructive" className="w-full">{choice.error}</Alert> : null}
    <Select value={choice.selected ?? ""} disabled={choice.changing} onValueChange={choice.select}>
      <ChatComposerModel select aria-label={MODEL_LABEL} title={MODEL_HINT}><SelectValue placeholder={choice.label}>{choice.label}</SelectValue></ChatComposerModel>
      <SelectContent>
        {models.map(model => <SelectItem key={model.ref} value={model.ref}>{model.label}</SelectItem>)}
        {!models.length ? <SelectItem value="unavailable" disabled>{EMPTY}</SelectItem> : null}
      </SelectContent>
    </Select>
  </div>;
}
