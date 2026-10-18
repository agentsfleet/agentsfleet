"use client";

import { useState } from "react";
import {
  Alert, Button, Input, Label, Select, SelectContent, SelectTrigger, SelectValue, SelectItem, Switch,
} from "@agentsfleet/design-system";
import { modelsForProvider, OPENAI_PROVIDER, uniqueModelIds, type LibraryModel } from "@/lib/api/model-library-types";
import { modelLabel } from "@/lib/models/display";
import { CATALOGUE_STATUS, type CatalogueStatus } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";

const CATALOGUE_LOADING_PLACEHOLDER = "Loading models…";
const SELECT_PLACEHOLDER = "Select a model";
const MODEL_ID_CLASS = "font-mono text-label leading-label text-muted-foreground";

export type ProviderModelSelectProps = {
  id: string;
  provider?: string;
  model: string;
  onModelChange: (value: string) => void;
  label?: string;
};

function LoadingModelSelect({ id, label }: { id: string; label: string }) {
  return (
    <Select disabled>
      <SelectTrigger id={id} aria-label={label}>
        <SelectValue placeholder={CATALOGUE_LOADING_PLACEHOLDER} />
      </SelectTrigger>
      <SelectContent />
    </Select>
  );
}

/** Catalogue names and exact identifiers, scoped to the selected provider. */
export default function ProviderModelSelect({
  id, provider, model, onModelChange, label = "Model",
}: ProviderModelSelectProps) {
  const { models, status, preloadAdvanced } = useModelCatalogue();
  const [showAdvanced, setShowAdvanced] = useState(false);
  const visible = models.filter((option) => showAdvanced || !option.advanced);
  const options = provider ? modelsForProvider(visible, provider) : uniqueModelIds(visible);
  const advancedId = `${id}-advanced`;
  const changeAdvanced = (checked: boolean) => {
    setShowAdvanced(checked);
    if (checked) preloadAdvanced();
    else if (models.some((option) => option.id === model && option.advanced)) onModelChange("");
  };
  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      <ModelControl id={id} label={label} model={model} onModelChange={onModelChange} options={options} status={status} />
      {provider === undefined || provider === OPENAI_PROVIDER ? (
        <div className="flex items-center gap-2">
          <Switch id={advancedId} checked={showAdvanced} onCheckedChange={changeAdvanced} />
          <Label htmlFor={advancedId}>Show advanced models</Label>
        </div>
      ) : null}
      {showAdvanced && status === CATALOGUE_STATUS.error ? (
        <Alert variant="warning">
          Could not load advanced models.
          <Button type="button" variant="ghost" onClick={preloadAdvanced}>Try again</Button>
        </Alert>
      ) : null}
    </div>
  );
}

function ModelControl({ id, label, model, onModelChange, options, status }: ProviderModelSelectProps & {
  label: string;
  options: LibraryModel[];
  status: CatalogueStatus;
}) {
  if (status === CATALOGUE_STATUS.loading) return <LoadingModelSelect id={id} label={label} />;
  if (options.length === 0) return (
    <Input id={id} value={model} onChange={(event) => onModelChange(event.target.value)}
      placeholder="Exact model ID" spellCheck={false} autoComplete="off" />
  );
  return (
    <Select value={model} onValueChange={onModelChange}>
      <SelectTrigger id={id} aria-label={label}>
        <SelectValue placeholder={SELECT_PLACEHOLDER} className="min-w-0 truncate">
          {model ? <ModelValue modelId={model} option={options.find((option) => option.id === model)} /> : null}
        </SelectValue>
      </SelectTrigger>
      <SelectContent>
        {options.map((option) => (
          <SelectItem key={option.id} value={option.id}>
            <ModelOption model={option} />
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}

function ModelOption({ model }: { model: LibraryModel }) {
  if (model.display_name === model.id) return model.id;
  return (
    <span className="flex min-w-0 flex-col">
      <span>{model.display_name}</span>{" "}
      <span className={MODEL_ID_CLASS}>{model.id}</span>
    </span>
  );
}

function ModelValue({ modelId, option }: { modelId: string; option: LibraryModel | undefined }) {
  const name = modelLabel(modelId, option?.display_name);
  if (name === modelId) return modelId;
  return <>{name} <span className={MODEL_ID_CLASS}>{modelId}</span></>;
}
