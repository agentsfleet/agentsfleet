"use client";

import { useRouter } from "next/navigation";
import { Alert, Button, Select, SelectContent, SelectItem, SelectValue } from "@agentsfleet/design-system";
import { ChatComposerModel } from "./ChatComposerFrame";
import { useFleetSetup } from "./FleetSetupWorkspace";
import type { useFleetModel, FleetModelOwner } from "./useFleetModel";

const MODEL_LABEL = "Fleet model";
const MODEL_HINT = "Saved model for future chat, webhook and scheduled runs";
const LOADING = "Loading models…";
const EMPTY = "No runtime models available";
const RETRY = "Retry";
const RELOAD = "Reload choices";
const RELOAD_FLEET = "Reload fleet";
const MORE = "Load more models";
const SAVING = "Saving model…";
const SAVED = "Model saved for future runs.";
const ignore = (): void => undefined;

export function FleetModelSelect({label}: {label: string}) {
  const setup = useFleetSetup();
  return setup?.modelOwner ? <ModelPicker key={`${setup.modelOwner.fleet}:${setup.modelOwner.revision}`} owner={setup.modelOwner}
    label={label} disabled={setup.editing} model={setup.modelState} /> : <ChatComposerModel>{label}</ChatComposerModel>;
}

function ModelPicker({owner, label, disabled, model}: {owner: FleetModelOwner; label: string; disabled: boolean; model: ReturnType<typeof useFleetModel>}) {
  const router = useRouter();
  const save = async (ref?: string): Promise<void> => {
    const saved = await (ref === undefined ? model.retry() : model.select(ref));
    if (saved) router.refresh();
  };
  return <div className="flex min-w-0 flex-1 flex-wrap items-center gap-sm">
    <Select value={model.confirmed?.ref ?? owner.model} disabled={disabled || model.locked}
      onOpenChange={open => {if (open && !model.ready) void model.load();}} onValueChange={ref => {void save(ref).catch(ignore);}}>
      <ChatComposerModel select aria-label={MODEL_LABEL} title={MODEL_HINT}><SelectValue>{model.confirmed?.label ?? label}</SelectValue></ChatComposerModel>
      <SelectContent>
        {model.models.map(item => <SelectItem key={item.ref} value={item.ref}>{item.label}</SelectItem>)}
        {!model.models.length ? <SelectItem value="unavailable" disabled>{model.error ?? (model.loading ? LOADING : EMPTY)}</SelectItem> : null}
      </SelectContent>
    </Select>
    {model.more && !model.inaccessible ? <Button variant="ghost" size="sm" disabled={model.loading || model.locked || disabled} onClick={() => {void model.load(true);}}>{MORE}</Button> : null}
    {model.saving || model.confirmed ? <output className="text-eyebrow text-muted-foreground">{model.saving ? SAVING : SAVED}</output> : null}
    {model.error ? <Alert variant="destructive" className="w-full">{model.error}
      {!model.inaccessible ? <Button variant="outline" size="sm" disabled={model.saving || model.loading} onClick={() => {
        if (model.reloadRequired) {try {router.refresh(); model.resetRefusal();} catch {ignore();}}
        else if (model.retryable) void save().catch(ignore); else void model.load();
      }}>{model.reloadRequired ? RELOAD_FLEET : model.retryable ? RETRY : RELOAD}</Button> : null}
    </Alert> : null}
  </div>;
}
