"use client";

import {
  Input,
  Label,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@usezombie/design-system";
import type { ModelCap } from "@/lib/api/model_caps";

export type Step2ModelProps = {
  catalogue: ModelCap[];
  model: string;
  onModelChange: (value: string) => void;
};

// Radix <Select> reserves the empty string, so the "no override" entry uses a
// sentinel that maps back to "" — submit then omits `model` and the backend
// falls back to the credential's stored model.
const USE_CREDENTIAL_MODEL = "__use_credential_model__";

/**
 * Step 2 of the self-managed wizard — choose the model. With the public
 * catalogue present this is a catalogue-backed picker (a free-typed unknown
 * model would 400 at PUT time). If the catalogue could not be fetched the field
 * degrades to a free-text input so the wizard still works.
 */
export default function Step2Model({ catalogue, model, onModelChange }: Step2ModelProps) {
  const hasCatalogue = catalogue.length > 0;

  // `core.model_caps` is keyed by (provider, model_id), so the same model_id
  // legitimately recurs across providers (e.g. claude-opus-4-8 on anthropic and
  // pioneer). This override picker is provider-agnostic — the backend resolves
  // the provider from the selected credential at PUT time and only needs the
  // bare model_id — so collapse the catalogue to one entry per id. Without this
  // the duplicate ids produce colliding React keys + duplicate Radix
  // <SelectItem> values and the page throws.
  const uniqueModels = Array.from(new Map(catalogue.map((m) => [m.id, m])).values());

  return (
    <div className="space-y-2">
      <Label htmlFor="model-override">Model</Label>
      {hasCatalogue ? (
        <Select
          value={model === "" ? USE_CREDENTIAL_MODEL : model}
          onValueChange={(v) => onModelChange(v === USE_CREDENTIAL_MODEL ? "" : v)}
        >
          <SelectTrigger id="model-override" aria-label="Model">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={USE_CREDENTIAL_MODEL}>Use the credential&apos;s model</SelectItem>
            {uniqueModels.map((m) => (
              <SelectItem key={m.id} value={m.id}>
                {m.id}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      ) : (
        <Input
          id="model-override"
          value={model}
          onChange={(e) => onModelChange(e.target.value)}
          placeholder="leave blank to use the credential's model field"
          spellCheck={false}
        />
      )}
      <p className="text-xs text-muted-foreground">
        Optional. Pick a model from the public catalogue, or leave it on the model stored with the
        credential.
      </p>
    </div>
  );
}
