"use client";

import { useState } from "react";
import { Input, Label, Select, SelectContent, SelectTrigger, SelectValue, SelectItem } from "@agentsfleet/design-system";
import { modelsForProvider, uniqueModelIds, type LibraryModel } from "@/lib/api/model-library-types";
import { modelLabel } from "@/lib/models/display";
import { CatalogueModelDetails } from "@/components/domain/CatalogueModelDetails";
import { CATALOGUE_STATUS, type CatalogueStatus } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { CatalogueBrowseControls } from "./CatalogueBrowseControls";

const CATALOGUE_LOADING_PLACEHOLDER = "Loading models…";
const SELECT_PLACEHOLDER = "Select a model";
const MODEL_ID_CLASS = "font-mono text-label leading-label text-muted-foreground";

export type ProviderModelSelectProps = {
  id: string; provider?: string; model: string;
  onModelChange: (value: string) => void; label?: string;
};

/** Names, evidence and exact identifiers, scoped to the selected provider. */
export default function ProviderModelSelect({ id, provider, model, onModelChange, label = "Model" }: ProviderModelSelectProps) {
  const { models, status } = useModelCatalogue();
  const [showAdvanced, setShowAdvanced] = useState(false);
  const available = provider ? modelsForProvider(models, provider) : uniqueModelIds(models);
  const options = available.filter((option) => showAdvanced || !option.advanced);
  const selected = available.find((option) => option.id === model);
  return <div className="space-y-sm">
    <Label htmlFor={id}>{label}</Label>
    <ModelControl id={id} label={label} model={model} onModelChange={onModelChange}
      options={options} selected={selected} status={status} />
    <CatalogueBrowseControls id={`${id}-advanced`} advanced={showAdvanced} onAdvancedChange={setShowAdvanced} />
    {selected ? <CatalogueModelDetails model={selected} /> : null}
  </div>;
}

function ModelControl({ id, label, model, onModelChange, options, selected, status }: ProviderModelSelectProps & {
  label: string; options: LibraryModel[]; selected: LibraryModel | undefined; status: CatalogueStatus;
}) {
  const loading = status === CATALOGUE_STATUS.loading;
  if (!loading && options.length === 0 && !selected) return (
    <Input id={id} value={model} onChange={(event) => onModelChange(event.target.value)}
      placeholder="Exact model ID" spellCheck={false} autoComplete="off" />
  );
  const retained = model !== "" && !options.some((option) => option.id === model);
  return <Select value={model} onValueChange={onModelChange} disabled={loading && options.length === 0}>
    <SelectTrigger id={id} aria-label={label}>
      <SelectValue placeholder={loading ? CATALOGUE_LOADING_PLACEHOLDER : SELECT_PLACEHOLDER} className="min-w-0 truncate">
        {model ? <ModelValue modelId={model} option={selected} /> : null}
      </SelectValue>
    </SelectTrigger>
    <SelectContent>
      {retained ? <SelectItem value={model}><ModelValue modelId={model} option={selected} /></SelectItem> : null}
      {options.map((option) => <SelectItem key={option.id} value={option.id}><ModelOption model={option} /></SelectItem>)}
    </SelectContent>
  </Select>;
}

function ModelOption({ model }: { model: LibraryModel }) {
  if (model.display_name === model.id) return model.id;
  return <span className="flex min-w-0 flex-col">
    <span>{model.display_name}</span>{" "}<span className={MODEL_ID_CLASS}>{model.id}</span>
  </span>;
}

function ModelValue({ modelId, option }: { modelId: string; option: LibraryModel | undefined }) {
  const name = modelLabel(modelId, option?.display_name);
  if (name === modelId) return modelId;
  return <>{name} <span className={MODEL_ID_CLASS}>{modelId}</span></>;
}
