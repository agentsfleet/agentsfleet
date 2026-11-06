"use client";

import type { Control } from "react-hook-form";
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Switch,
} from "@agentsfleet/design-system";
import type { PolicyFormValues } from "./PolicyFields";

export function PolicyDurableTasksField({ control }: { control: Control<PolicyFormValues> }) {
  return <FormField control={control} name="durable_tasks_enabled" render={({ field }) => (
    <FormItem>
      <div className="flex items-center justify-between gap-md">
        <FormLabel>Durable tasks</FormLabel>
        <FormControl>
          <Switch checked={field.value} onCheckedChange={field.onChange} onBlur={field.onBlur} name={field.name} ref={field.ref} />
        </FormControl>
      </div>
      <FormDescription>
        Let this runner continue tasks across execution sessions. Changes apply to new work; work already running continues.
      </FormDescription>
      <FormMessage />
    </FormItem>
  )} />;
}
