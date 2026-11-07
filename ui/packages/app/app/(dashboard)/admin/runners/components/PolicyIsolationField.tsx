"use client";

import { useId } from "react";
import type { Control } from "react-hook-form";
import { CircleHelpIcon } from "lucide-react";
import {
  FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  OptionCard, RadioGroup,
} from "@agentsfleet/design-system";
import { SANDBOX_TIERS, SANDBOX_TIER_DESCRIPTIONS, SANDBOX_TIER_LABELS } from "@/lib/api/runners-types";
import type { PolicyFormValues } from "./PolicyFields";

const ISOLATION_ASSIGNMENT_DESCRIPTION =
  "A host that cannot enforce the assigned tier is degraded and receives no work.";

export function PolicyIsolationField({ control }: { control: Control<PolicyFormValues> }) {
  const labelId = useId();
  return <FormField control={control} name="sandbox_tier" render={({ field }) => (
    <FormItem>
      <FormLabel id={labelId}>Isolation</FormLabel>
      <FormControl>
        <RadioGroup value={field.value} onValueChange={field.onChange} onBlur={field.onBlur}
          ref={field.ref} name={field.name} aria-labelledby={labelId} className="sm:grid-cols-3">
          {SANDBOX_TIERS.map(tier => <OptionCard key={tier} value={tier}
            label={SANDBOX_TIER_LABELS[tier]} description={SANDBOX_TIER_DESCRIPTIONS[tier]}
            className="p-md motion-reduce:transition-none" />)}
        </RadioGroup>
      </FormControl>
      <FormDescription>
        {ISOLATION_ASSIGNMENT_DESCRIPTION}{" "}
        <a href="https://docs.agentsfleet.net/runners" target="_blank" rel="noopener noreferrer"
          className="inline-flex items-center gap-1 text-pulse underline-offset-2 hover:underline focus-visible:underline">
          <CircleHelpIcon size={14} aria-hidden="true" />
          Learn more<span className="sr-only"> about runner policy (opens in a new tab)</span>
        </a>
      </FormDescription>
      <FormMessage />
    </FormItem>
  )} />;
}
