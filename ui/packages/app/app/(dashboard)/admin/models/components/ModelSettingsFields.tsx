"use client";

import { useEffect, useState } from "react";
import type { ModelSettingsValues } from "./model-settings";
import { useFormContext, useWatch } from "react-hook-form";
import { FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  Accordion, AccordionContent, AccordionItem, AccordionTrigger, Button, SectionHeader,
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

export function ModelSettingsFields({ compact = false }: { compact?: boolean }) {
  const form = useFormContext<ModelSettingsValues>();
  const [expanded, setExpanded] = useState(compact ? [] : ["catalogue", "pricing"]);
  const submits = form.formState.submitCount;
  useEffect(() => { if (submits > 0) setExpanded(["catalogue", "pricing"]); }, [submits]);
  return <>
    <InferenceFields />
    <Accordion type="multiple" value={expanded} onValueChange={setExpanded}>
      <AccordionItem value="catalogue">
        <AccordionTrigger>Catalogue details</AccordionTrigger>
        <AccordionContent>
          <div className="grid gap-md sm:grid-cols-2">
            {METADATA_FIELDS.map(([name, label]) => <SettingsTextField key={name} name={name} label={label} />)}
          </div>
          <AdvancedModelField />
        </AccordionContent>
      </AccordionItem>
      <AccordionItem value="pricing">
        <AccordionTrigger>Pricing</AccordionTrigger>
        <AccordionContent><PriceScheduleFields /></AccordionContent>
      </AccordionItem>
    </Accordion>
    <p className="text-sm text-muted-foreground">Save changes applies the connection, catalogue details and all pricing tiers together.</p>
  </>;
}

function AdvancedModelField() {
  const form = useFormContext<ModelSettingsValues>();
  return <FormField control={form.control} name="advanced" render={({ field }) => (
    <FormItem className="mt-lg rounded-lg border p-md">
      <div className="flex items-center justify-between gap-md">
        <FormLabel>Advanced model</FormLabel>
        <FormControl><Switch checked={field.value} onCheckedChange={field.onChange} /></FormControl>
      </div>
      <FormDescription>Hide this model from the standard picker. Users can show it by choosing Show advanced models.</FormDescription><FormMessage />
    </FormItem>
  )} />;
}

function InferenceFields() {
  const form = useFormContext<ModelSettingsValues>();
  const update = useWatch({ control: form.control, name: "update_inference" });
  const [hadConnection] = useState(update);
  return <>
    <SectionHeader as="p">Connection</SectionHeader>
    <p className="text-sm text-muted-foreground">The priming script fills these settings for catalogue models. Credentials are added when you make a model the platform default.</p>
    {!update ? <div className="space-y-sm">
      <p className="text-sm text-muted-foreground">No connection configured. Saving catalogue details or prices leaves the connection unchanged.</p>
      <Button type="button" variant="secondary" onClick={() => form.setValue("update_inference", true)}>Configure connection</Button>
    </div> : null}
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
      {!hadConnection ? <Button type="button" variant="ghost" onClick={() => {
        form.resetField("api");
        for (const [name] of INFERENCE_FIELDS) form.resetField(name);
        form.setValue("update_inference", false);
        form.clearErrors(["api", ...INFERENCE_FIELDS.map(([name]) => name)]);
      }}>Leave connection unchanged</Button> : null}
    </> : null}
  </>;
}
