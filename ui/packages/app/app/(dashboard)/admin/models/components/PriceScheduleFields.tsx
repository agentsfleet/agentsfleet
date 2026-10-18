"use client";

import { useFieldArray, useFormContext } from "react-hook-form";
import { Accordion, AccordionContent, AccordionItem, AccordionTrigger, Alert, Button, SectionHeader } from "@agentsfleet/design-system";
import { CATALOGUE_LIMITS } from "@/lib/api/catalogue-types";
import { EMPTY_PRICE, type ModelSettingsValues } from "./model-settings";
import { SettingsTextField } from "./SettingsTextField";

const PRICE_FIELDS = [
  ["prompt_tokens_above", "Prompt tokens above", "0 is the base tier. A larger threshold prices the whole prompt above it."],
  ["effective_from", "Effective from", "An instant with timezone, for example 2026-12-01T00:00:00Z."],
  ["effective_until", "Effective until", "Leave blank for an open date range. The end is exclusive."],
  ["input_usd", "Input ($/1M)", ""], ["cached_usd", "Cached ($/1M)", ""], ["output_usd", "Output ($/1M)", ""],
  ["write_usd", "Cache write ($/1M)", "Leave blank when unknown."],
  ["write_5m_usd", "Cache write, 5-minute ($/1M)", "Leave blank when unknown."],
  ["write_1h_usd", "Cache write, 1-hour ($/1M)", "Leave blank when unknown."],
  ["source_url", "Price source URL", ""], ["verified_at", "Price verified at", "Use an instant with timezone."],
  ["conditions", "Conditions", "Provider scope and exclusions; may be empty."],
] as const;
const UNKNOWN_NOTE = "Prices are US dollars per million tokens. Blank cache-write prices are unknown; enter 0 only for a confirmed free rate.";

export function PriceScheduleFields() {
  const form = useFormContext<ModelSettingsValues>();
  const { fields, append, remove } = useFieldArray({ control: form.control, name: "prices" });
  const error = form.formState.errors.prices;
  const message = error?.root?.message ?? error?.message;
  return <div className="space-y-md">
    <SectionHeader as="p">Price schedule</SectionHeader>
    <Alert>{UNKNOWN_NOTE}</Alert>
    {message ? <Alert variant="destructive">{message}</Alert> : null}
    <Accordion type="multiple" defaultValue={["price-0"]}>
      {fields.map((field, index) => <AccordionItem key={field.id} value={`price-${index}`}>
        <AccordionTrigger>Price {index + 1}</AccordionTrigger>
        <AccordionContent>
          <div className="grid gap-md sm:grid-cols-2">
            {PRICE_FIELDS.map(([name, label, description]) => <SettingsTextField key={name}
              name={`prices.${index}.${name}`} label={label} description={description} />)}
          </div>
          <Button type="button" variant="destructive" className="mt-md" disabled={fields.length === 1}
            onClick={() => remove(index)} aria-label={`Remove price ${index + 1}`}>Remove price</Button>
        </AccordionContent>
      </AccordionItem>)}
    </Accordion>
    <Button type="button" variant="secondary" disabled={fields.length >= CATALOGUE_LIMITS.prices}
      onClick={() => append({ ...EMPTY_PRICE })}>Add price</Button>
  </div>;
}
