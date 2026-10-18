"use client";

import type { ModelSettingsValues } from "./model-settings";
import { useFormContext, useWatch } from "react-hook-form";
import { FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue, Switch } from "@agentsfleet/design-system";
import { INFERENCE_API } from "@/lib/api/admin-model-library-types";
import { SettingsTextField } from "./SettingsTextField";
import { PriceScheduleFields } from "./PriceScheduleFields";

const METADATA_FIELDS = [
  ["display_name", "Catalogue display name"], ["context_cap_tokens", "Context window (tokens)"],
  ["context_source_url", "Context source URL"], ["context_verified_at", "Context verified at"],
] as const;
const INFERENCE_FIELDS = [
  ["base_url", "Inference base URL"], ["max_output_tokens", "Model output limit (tokens)"],
  ["dialect", "Dialect (optional)"],
] as const;
const API_LABELS = {
  [INFERENCE_API.chat]: "Chat completions", [INFERENCE_API.responses]: "Responses",
  [INFERENCE_API.messages]: "Messages", [INFERENCE_API.gemini]: "Gemini", [INFERENCE_API.ollama]: "Ollama",
} as const;

export function ModelSettingsFields() {
  return <>
    <div className="grid gap-md sm:grid-cols-2">
      {METADATA_FIELDS.map(([name, label]) => <SettingsTextField key={name} name={name} label={label} />)}
    </div>
    <SettingsSwitch name="advanced" label="Advanced model" description="Shown in the picker only after an explicit advanced-model request." />
    <PriceScheduleFields />
    <InferenceFields />
  </>;
}

function SettingsSwitch({ name, label, description }: {
  name: "advanced" | "update_inference"; label: string; description: string;
}) {
  const form = useFormContext<ModelSettingsValues>();
  return <FormField control={form.control} name={name} render={({ field }) => (
    <FormItem><FormLabel>{label}</FormLabel>
      <FormControl><Switch checked={field.value} onCheckedChange={field.onChange} /></FormControl>
      <FormDescription>{description}</FormDescription><FormMessage />
    </FormItem>
  )} />;
}

function InferenceFields() {
  const form = useFormContext<ModelSettingsValues>();
  const update = useWatch({ control: form.control, name: "update_inference" });
  return <>
    <SettingsSwitch name="update_inference" label="Write inference settings"
      description="When off, editing preserves the existing settings; a new model remains unavailable for inference." />
    {update ? <>
      <FormField control={form.control} name="api" render={({ field }) => (
        <FormItem><FormLabel>Inference API</FormLabel>
          <Select value={field.value} onValueChange={field.onChange}>
            <FormControl><SelectTrigger><SelectValue /></SelectTrigger></FormControl>
            <SelectContent>{Object.entries(API_LABELS).map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectContent>
          </Select><FormMessage />
        </FormItem>
      )} />
      <div className="grid gap-md sm:grid-cols-2">
        {INFERENCE_FIELDS.map(([name, label]) => <SettingsTextField key={name} name={name} label={label} />)}
      </div>
    </> : null}
  </>;
}
