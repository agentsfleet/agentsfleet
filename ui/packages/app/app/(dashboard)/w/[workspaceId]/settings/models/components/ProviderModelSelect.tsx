"use client";

import {
  Input,
  Label,
  Select,
  SelectContent,
  SelectTrigger,
  SelectValue,
  SelectItem,
} from "@agentsfleet/design-system";
import { modelsForProvider, uniqueModelIds } from "@/lib/api/model-library-types";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { knownModelsFor } from "../lib/known-models";
import { modelLabel } from "@/lib/models/display";

const CATALOGUE_LOADING_PLACEHOLDER = "Loading models…";
const SELECT_PLACEHOLDER = "Select a model";
// An id beside or under its name reads as a quiet reference, a size below the name.
const MODEL_ID_CLASS = "font-mono text-label leading-label text-muted-foreground";

export type ProviderModelSelectProps = {
  id: string;
  /** Scope the picker to one provider's models; omit for a provider-agnostic id list. */
  provider?: string;
  model: string;
  onModelChange: (value: string) => void;
  label?: string;
};

/**
 * Held while the catalogue is in flight (it loads on dialog-open intent, so a
 * cold open renders this for the round-trip). A disabled Select rather than
 * letting the models array decide the control: an empty array would mount the
 * free-text Input and then swap it for a Select when the catalogue lands —
 * replacing the control mid-interaction, dropping focus, and visually
 * orphaning anything already typed.
 */
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

/**
 * Model picker with three tiers: the admin-managed, priced catalogue first
 * (ModelCatalogueProvider) — a free-typed unknown model there would 400 at
 * PUT time, so a catalogue hit is a constrained <Select>; when the catalogue
 * has no rows for this provider, the small static known-models list
 * (lib/known-models.ts) fills the same <Select> shape as a plain autocomplete
 * convenience; only when NEITHER covers the provider does this degrade to a
 * free-text input. Provider-scoped because core.model_library is keyed by
 * (provider, model_id).
 */
export default function ProviderModelSelect({
  id,
  provider,
  model,
  onModelChange,
  label = "Model",
}: ProviderModelSelectProps) {
  const { models, status } = useModelCatalogue();
  const catalogueOptions = provider ? modelsForProvider(models, provider) : uniqueModelIds(models);
  const optionIds =
    catalogueOptions.length > 0
      ? catalogueOptions.map((m) => m.id)
      : provider
        ? knownModelsFor(provider)
        : [];

  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      {status === CATALOGUE_STATUS.loading ? (
        <LoadingModelSelect id={id} label={label} />
      ) : optionIds.length > 0 ? (
        <Select value={model} onValueChange={onModelChange}>
          <SelectTrigger id={id} aria-label={label}>
            <SelectValue placeholder={SELECT_PLACEHOLDER} className="min-w-0 truncate">
              {model ? <ModelValue modelId={model} /> : null}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            {optionIds.map((m) => (
              <SelectItem key={m} value={m}>
                <ModelOption modelId={m} />
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : (
        <Input
          id={id}
          value={model}
          onChange={(e) => onModelChange(e.target.value)}
          placeholder="claude-sonnet-4-6"
          spellCheck={false}
          autoComplete="off"
        />
      )}
    </div>
  );
}

// An option names the model the way the tables do, with the id the entry
// stores on its own line, so a long id never pushes the name off the list.
function ModelOption({ modelId }: { modelId: string }) {
  const name = modelLabel(modelId);
  if (name === modelId) return modelId;
  return (
    <span className="flex min-w-0 flex-col">
      <span>{name}</span>{" "}
      <span className={MODEL_ID_CLASS}>{modelId}</span>
    </span>
  );
}

// The closed picker keeps one line: the name, then the id until the trigger
// runs out of room.
function ModelValue({ modelId }: { modelId: string }) {
  const name = modelLabel(modelId);
  if (name === modelId) return modelId;
  return (
    <>
      {name} <span className={MODEL_ID_CLASS}>{modelId}</span>
    </>
  );
}
