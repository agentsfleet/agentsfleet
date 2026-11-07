"use client";

import { useId } from "react";
import { useWatch, type Control } from "react-hook-form";
import { Globe2Icon, ShieldCheckIcon, UnplugIcon } from "lucide-react";
import {
  FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  Input, OptionCard, RadioGroup,
} from "@agentsfleet/design-system";
import { NETWORK_POLICIES, NETWORK_POLICY_DESCRIPTIONS, NETWORK_POLICY_LABELS } from "@/lib/api/runners-types";
import type { PolicyFormValues } from "./PolicyFields";

const NETWORK_ICONS = { allow_all: Globe2Icon, allow_list_egress: ShieldCheckIcon, deny_all_egress: UnplugIcon };
const NETWORK_COLORS = { allow_all: "text-info", allow_list_egress: "text-pulse", deny_all_egress: "text-warning" };

export function PolicyNetworkFields({ control }: { control: Control<PolicyFormValues> }) {
  const labelId = useId();
  const network = useWatch({control, name: "network_policy"});
  return <>
    <FormField control={control} name="network_policy" render={({ field }) => (
      <FormItem>
        <FormLabel id={labelId}>Network policy</FormLabel>
        <FormControl>
          <RadioGroup value={field.value} onValueChange={field.onChange} onBlur={field.onBlur}
            ref={field.ref} name={field.name} aria-labelledby={labelId}>
            {NETWORK_POLICIES.map(policy => {
              const Icon = NETWORK_ICONS[policy];
              return <OptionCard key={policy} value={policy} label={NETWORK_POLICY_LABELS[policy]}
                description={NETWORK_POLICY_DESCRIPTIONS[policy]} icon={<Icon size={16} className={NETWORK_COLORS[policy]} aria-hidden="true" />}
                className="gap-md p-md motion-reduce:transition-none" />;
            })}
          </RadioGroup>
        </FormControl>
        <FormMessage />
      </FormItem>
    )} />
    <FormField control={control} name="registry_allowlist" render={({ field }) => (
      <FormItem>
        <FormLabel>Package download sites (optional)</FormLabel>
        <FormControl><Input placeholder="registry.npmjs.org, pypi.org" autoComplete="off" {...field} disabled={network !== "allow_list_egress"} /></FormControl>
        <FormDescription>Used only with approved destinations. Blank uses the standard package sites. Custom hosts replace that list.</FormDescription>
        <FormMessage />
      </FormItem>
    )} />
  </>;
}
