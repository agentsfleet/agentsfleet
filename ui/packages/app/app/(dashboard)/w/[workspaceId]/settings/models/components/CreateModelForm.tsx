"use client";

import { useId, useState } from "react";
import { Alert, Button, DialogFooter, Input, Label, Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Spinner, Switch } from "@agentsfleet/design-system";
import type { Secret } from "@/lib/api/secrets";
import { providerLabel, uniqueProviders } from "@/lib/api/model-library-types";
import { OPENAI_COMPATIBLE_PROVIDER, SECRET_FIELD } from "@/lib/types";
import { BASE_URL_NOT_HTTPS, isHttpsUrl } from "../lib/custom-endpoint";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import ProviderModelSelect from "./ProviderModelSelect";
import { ModelCredentialFields } from "./ModelCredentialFields";
import { useModelCreation } from "./use-model-creation";
import { compatibleCredentials, creationActionLabel, KEY_MODE, type ModelFormValues } from "./model-creation";

type Props = {
  workspaceId: string; secrets: readonly Secret[];
  onCreated: () => void; onSecretsChanged: () => void;
  onCancel: () => void; onComplete: () => void;
  onPendingChange: (pending: boolean) => void;
};
const INITIAL_VALUES: ModelFormValues = {
  provider: "", model: "", keyName: "", baseUrl: "", apiKey: "", mode: KEY_MODE.new, savedRef: "", activate: false,
};

export function CreateModelForm({workspaceId, secrets, onCreated, onSecretsChanged, onCancel, onComplete, onPendingChange}: Props) {
  const id = useId();
  const [values, setValues] = useState(INITIAL_VALUES);
  const [validationError, setValidationError] = useState<string | null>(null);
  const catalogue = useModelCatalogue();
  const providers = uniqueProviders(catalogue.models).filter(provider => provider !== OPENAI_COMPATIBLE_PROVIDER);
  const catalogueLoading = catalogue.status === CATALOGUE_STATUS.idle || catalogue.status === CATALOGUE_STATUS.loading;
  const creation = useModelCreation({onCreated, onSecretsChanged, clearKey: () => change({apiKey: ""}), onComplete}, onPendingChange);
  const custom = values.provider.trim() === OPENAI_COMPATIBLE_PROVIDER;
  const available = compatibleCredentials(secrets, values.provider.trim());
  const heldName = secrets.some(secret => secret.name === values.keyName.trim());
  const validCredential = values.mode === KEY_MODE.saved
    ? available.some(secret => secret.name === values.savedRef)
    : values.keyName.trim() !== "" && !heldName && (custom ? values.baseUrl.trim() !== "" : values.apiKey.trim() !== "");
  const canCreate = workspaceId !== "" && values.provider.trim() !== "" && values.model.trim() !== "" && validCredential;
  const error = validationError ?? creation.state.error;
  const locked = creation.pending || creation.state.stage !== "editing";

  function change(patch: Partial<ModelFormValues>) {
    setValues(previous => ({...previous, ...patch}));
    setValidationError(null);
  }

  function submit() {
    if (creation.state.stage === "storage_unconfirmed") {
      creation.reset();
      change({mode: available.length ? KEY_MODE.saved : KEY_MODE.new, savedRef: "", keyName: "", apiKey: ""});
      return;
    }
    if (creation.state.stage !== "editing") { creation.retry(); return; }
    if (!canCreate) return;
    const adding = values.mode === KEY_MODE.new;
    if (adding && custom && !isHttpsUrl(values.baseUrl)) { setValidationError(BASE_URL_NOT_HTTPS); return; }
    const request = {workspaceId, secretRef: adding ? values.keyName.trim() : values.savedRef, modelId: values.model.trim(), activate: values.activate};
    const data: Record<string, unknown> = {[SECRET_FIELD.provider]: values.provider.trim()};
    if (custom) data[SECRET_FIELD.baseUrl] = values.baseUrl.trim();
    if (values.apiKey.trim()) data[SECRET_FIELD.apiKey] = values.apiKey.trim();
    creation.create(request, adding ? data : undefined);
  }

  return (
    <form action={submit} className="space-y-2xl" aria-busy={creation.pending}>
      <fieldset disabled={locked} className="min-w-0 space-y-xl border-0 p-0">
        <div className="grid min-w-0 grid-cols-1 gap-xl sm:grid-cols-2">
          <div className="space-y-sm">
            <Label htmlFor={`${id}-provider`}>Provider</Label>
            {catalogueLoading || providers.length ? <Select value={values.provider} disabled={catalogueLoading} onValueChange={provider => change({provider, model: "", savedRef: "", apiKey: ""})}>
              <SelectTrigger id={`${id}-provider`} aria-label="Provider"><SelectValue placeholder={catalogueLoading ? "Loading providers…" : "Select a provider"} /></SelectTrigger>
              <SelectContent>{providers.map(provider => <SelectItem key={provider} value={provider}>{providerLabel(provider)}</SelectItem>)}<SelectItem value={OPENAI_COMPATIBLE_PROVIDER}>{providerLabel(OPENAI_COMPATIBLE_PROVIDER)}</SelectItem></SelectContent>
            </Select> : <Input id={`${id}-provider`} value={values.provider} onChange={event => change({provider: event.target.value, model: "", savedRef: "", apiKey: ""})} placeholder="Provider ID" autoComplete="off" spellCheck={false} />}
          </div>
          <ProviderModelSelect id={`${id}-model`} provider={values.provider || undefined} model={values.model} onModelChange={model => change({model})} details="disclosure" />
        </div>
        <ModelCredentialFields id={id} values={values} secrets={secrets} onChange={change} />
        <div className="flex items-start gap-lg">
          <Switch id={`${id}-active`} checked={values.activate} onCheckedChange={activate => change({activate})} />
          <div className="space-y-xs"><Label htmlFor={`${id}-active`}>Make active</Label><p className="text-eyebrow text-muted-foreground">Use this as your default model for new work.</p></div>
        </div>
      </fieldset>
      {error && !creation.pending ? <Alert variant={creation.state.stage.endsWith("unconfirmed") ? "warning" : "destructive"}>{error}</Alert> : null}
      <DialogFooter>
        <Button type="button" variant="outline" disabled={creation.pending} onClick={onCancel}>Cancel</Button>
        <Button type="submit" disabled={creation.pending || (creation.state.stage === "editing" && !canCreate)}>
          {creation.pending ? <Spinner size="sm" srLabel="Saving model" /> : null}
          {creation.pending ? "Saving…" : creationActionLabel(creation.state.stage)}
        </Button>
      </DialogFooter>
    </form>
  );
}
