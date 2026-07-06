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

export type ProviderModelSelectProps = {
  id: string;
  /** Scope the picker to one provider's models; omit for a provider-agnostic id list. */
  provider?: string;
  model: string;
  onModelChange: (value: string) => void;
  label?: string;
};

/**
 * Catalogue-backed model picker reading the once-per-session catalogue from
 * ModelCatalogueProvider. A free-typed unknown model would 400 at PUT time, so
 * with a catalogue present this is a constrained <Select>; when the catalogue is
 * empty (fetch failed / provider not covered) it degrades to a free-text input
 * so the form still works. Provider-scoped because core.model_caps is keyed by
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
  const options = provider ? modelsForProvider(models, provider) : uniqueModelIds(models);

  return (
    <div className="space-y-2">
      <Label htmlFor={id}>{label}</Label>
      {options.length > 0 ? (
        <Select value={model} onValueChange={onModelChange}>
          <SelectTrigger id={id} aria-label={label}>
            <SelectValue placeholder="Select a model" />
          </SelectTrigger>
          <SelectContent>
            {options.map((m) => (
              <SelectItem key={m.id} value={m.id}>
                {m.id}
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
