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
import { modelsForProvider, uniqueModelIds } from "@/lib/api/model_library";
import { CATALOGUE_STATUS } from "./catalogue-status";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { knownModelsFor } from "../lib/known-models";

const CATALOGUE_LOADING_PLACEHOLDER = "Loading models…";

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
            <SelectValue placeholder="Select a model" />
          </SelectTrigger>
          <SelectContent>
            {optionIds.map((m) => (
              <SelectItem key={m} value={m}>
                {m}
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
