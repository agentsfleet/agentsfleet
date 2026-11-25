"use client";

import { Alert, Select, SelectContent, SelectItem, SelectValue } from "@agentsfleet/design-system";
import { ChatComposerModel } from "../shared/chat/ChatComposerFrame";
import { useFleetModelChoice } from "./FleetModelContext";

const MODEL_LABEL = "Fleet model";
const MODEL_HINT = "Choose the model this fleet uses";
const EMPTY = "No models available";
const SAVING = "Saving model…";

export function FleetModelSelect({label}: {label: string}) {
  const choice = useFleetModelChoice();
  if (!choice) return <ChatComposerModel>{label}</ChatComposerModel>;
  const models = choice.models.filter(model => model.eligibility.runtime);
  return <div className="flex min-w-0 max-w-full flex-wrap items-center gap-sm">
    <Select value={choice.selected ?? ""} disabled={choice.changing} onValueChange={choice.select}>
      <ChatComposerModel select aria-label={MODEL_LABEL} title={MODEL_HINT}><SelectValue placeholder={choice.label}>{choice.label}</SelectValue></ChatComposerModel>
      <SelectContent align="start" className="max-w-[var(--radix-select-content-available-width)] min-w-[var(--radix-select-trigger-width)] motion-reduce:animate-none">
        {models.map(model => <SelectItem key={model.ref} value={model.ref} className="min-h-9 wrap-anywhere text-label leading-label pointer-coarse:min-h-11">{model.label}</SelectItem>)}
        {!models.length ? <SelectItem value="unavailable" disabled>{EMPTY}</SelectItem> : null}
      </SelectContent>
    </Select>
    {choice.changing ? <output className="text-eyebrow leading-eyebrow text-muted-foreground">{SAVING}</output> : null}
    {choice.error ? <Alert variant="destructive" className="w-full wrap-anywhere px-lg py-md">{choice.error}</Alert> : null}
  </div>;
}
