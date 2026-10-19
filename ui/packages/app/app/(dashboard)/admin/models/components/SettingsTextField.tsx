"use client";

import type { FieldPathByValue } from "react-hook-form";
import type { ModelSettingsValues } from "./model-settings";
import { useFormContext } from "react-hook-form";
import { FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage, Input } from "@agentsfleet/design-system";

export function SettingsTextField({ name, label, description }: {
  name: FieldPathByValue<ModelSettingsValues, string>; label: string; description?: string;
}) {
  const form = useFormContext<ModelSettingsValues>();
  return <FormField control={form.control} name={name} render={({ field }) => (
    <FormItem><FormLabel>{label}</FormLabel>
      <FormControl><Input name={field.name} ref={field.ref} onBlur={field.onBlur} onChange={field.onChange}
        value={field.value} autoComplete="off" /></FormControl>
      {description ? <FormDescription>{description}</FormDescription> : null}<FormMessage />
    </FormItem>
  )} />;
}

