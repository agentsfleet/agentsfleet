"use client";

import { useFormContext } from "react-hook-form";
import { FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  Input, Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@agentsfleet/design-system";
import { INFERENCE_API } from "@/lib/api/admin-model-library-types";
import type { ModelSettingsValues } from "./model-settings";

const TEXT_FIELDS = [
  ["context_cap_tokens", "Context window (tokens)", "number"],
  ["base_url", "Inference base URL", "url"],
  ["max_output_tokens", "Model output limit (tokens)", "number"],
  ["dialect", "SDK dialect (optional)", "text"],
] as const;
const RATE_FIELDS = [["input_usd", "Input"], ["cached_usd", "Cached"], ["output_usd", "Output"]] as const;
const API_LABELS = { [INFERENCE_API.chat]: "Chat completions", [INFERENCE_API.responses]: "Responses", [INFERENCE_API.messages]: "Messages" } as const;

export function ModelSettingsFields() {
  const form = useFormContext<ModelSettingsValues>();
  return <>
    <FormField control={form.control} name="api" render={({ field }) => (
      <FormItem><FormLabel>Inference API</FormLabel>
        <Select value={field.value} onValueChange={field.onChange}>
          <FormControl><SelectTrigger><SelectValue /></SelectTrigger></FormControl>
          <SelectContent>{Object.entries(API_LABELS).map(([value, label]) => <SelectItem key={value} value={value}>{label}</SelectItem>)}</SelectContent>
        </Select><FormMessage />
      </FormItem>
    )} />
    {TEXT_FIELDS.map(([name, label, type]) => <FormField key={name} control={form.control} name={name} render={({ field }) => (
      <FormItem><FormLabel>{label}</FormLabel>
        <FormControl><Input type={type} autoComplete="off" {...(type === "number" ? { min: 1, step: 1 } : {})} {...field} /></FormControl>
        {name === "dialect" ? <FormDescription>Leave blank for the standard API. The server validates supported SDK dialects.</FormDescription> : null}
        <FormMessage />
      </FormItem>
    )} />)}
    <div className="grid grid-cols-1 gap-md sm:grid-cols-3">
      {RATE_FIELDS.map(([name, label]) => <FormField key={name} control={form.control} name={name} render={({ field }) => (
        <FormItem><FormLabel>{label} ($/1M)</FormLabel><FormControl><Input type="number" min={0} step="any" {...field} /></FormControl><FormMessage /></FormItem>
      )} />)}
    </div>
  </>;
}
