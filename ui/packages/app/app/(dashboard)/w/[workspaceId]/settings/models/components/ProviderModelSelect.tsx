"use client";

import {
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agentsfleet/design-system";
import { modelsForProvider, uniqueModelIds } from "@/lib/api/model_caps";
import { useModelCatalogue } from "./ModelCatalogueProvider";
import { knownModelsFor } from "../lib/known-models";

export type ProviderModelSelectProps = {
  id: string;
  /** Scope the picker to one provider's models; omit for a provider-agnostic id list. */
  provider?: string;
  model: string;
  onModelChange: (value: string) => void;
  label?: string;
};

/**
 * Model picker with three tiers: the admin-managed, priced catalogue first
 * (ModelCatalogueProvider) — a free-typed unknown model there would 400 at
 * PUT time, so a catalogue hit is a constrained <Select>; when the catalogue
 * has no rows for this provider, the small static known-models list
 * (lib/known-models.ts) fills the same <Select> shape as a plain autocomplete
 * convenience; only when NEITHER covers the provider does this degrade to a
 * free-text input. Provider-scoped because core.model_caps is keyed by
 * (provider, model_id).
 */
export default function ProviderModelSelect({
  id,
  provider,
  model,
  onModelChange,
  label = "Model",
}: ProviderModelSelectProps) {
  const { models } = useModelCatalogue();
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
      {optionIds.length > 0 ? (
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
