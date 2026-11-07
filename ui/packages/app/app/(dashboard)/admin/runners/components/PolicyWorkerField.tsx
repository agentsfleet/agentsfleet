"use client";

import type { Control } from "react-hook-form";
import {
  FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage, Input,
} from "@agentsfleet/design-system";
import { MAX_WORKER_COUNT, MIN_WORKER_COUNT } from "@/lib/api/runners-types";
import type { PolicyFormValues } from "./PolicyFields";

export function PolicyWorkerField({ control }: { control: Control<PolicyFormValues> }) {
  return <FormField control={control} name="worker_count" render={({ field }) => (
    <FormItem>
      <div className="flex items-center justify-between gap-lg">
        <div className="space-y-xs">
          <FormLabel>Workers</FormLabel>
          <p className="text-body-sm text-muted-foreground">Run fleet work in parallel.</p>
        </div>
        <FormControl>
          <Input type="number" inputMode="numeric" min={MIN_WORKER_COUNT} max={MAX_WORKER_COUNT}
            autoComplete="off" className="w-20 shrink-0 tabular-nums" {...field} />
        </FormControl>
      </div>
      <FormDescription>Choose {MIN_WORKER_COUNT}–{MAX_WORKER_COUNT} workers. Increasing beyond the runner’s startup worker count requires a runner restart.</FormDescription>
      <FormMessage />
    </FormItem>
  )} />;
}
