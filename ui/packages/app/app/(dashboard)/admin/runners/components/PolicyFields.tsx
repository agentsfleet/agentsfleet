"use client";

import { useId } from "react";
import { z } from "zod";
import type { Control } from "react-hook-form";
import {
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  OptionCard,
  RadioGroup,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@agentsfleet/design-system";
import {
  DEFAULT_ASSIGNED_NETWORK_POLICY,
  DEFAULT_WORKER_COUNT,
  MAX_WORKER_COUNT,
  MIN_WORKER_COUNT,
  NETWORK_POLICIES,
  NETWORK_POLICY_DESCRIPTIONS,
  NETWORK_POLICY_LABELS,
  SANDBOX_TIERS,
  SANDBOX_TIER_DESCRIPTIONS,
  SANDBOX_TIER_LABELS,
  parseRegistryAllowlist,
  type AssignedPolicy,
  type SandboxTier,
} from "@/lib/api/runners";

// The four-field ASSIGNMENT section, shared verbatim by AddRunnerDialog
// (enrollment) and EditPolicyDialog (re-assignment) so the two surfaces can
// never drift on what a policy is. Every description reads as an assignment
// the host must satisfy — the host applies exactly this and reports what it
// can actually enforce; it never declares its own policy.

export const DEFAULT_ASSIGNED_SANDBOX_TIER: SandboxTier = "landlock_full";

export const ISOLATION_ASSIGNMENT_DESCRIPTION =
  "The isolation this host must enforce. Assigned by you, applied by the host — a host that cannot deliver it is marked degraded and receives no work.";
const NETWORK_ASSIGNMENT_LABEL = "Network policy";
const REGISTRY_ASSIGNMENT_LABEL = "Registry allowlist (optional)";
const REGISTRY_ASSIGNMENT_DESCRIPTION =
  "Comma-separated registry hosts merged into each lease's egress allowlist. Empty = the runner's default registry set.";
const WORKERS_ASSIGNMENT_LABEL = "Workers";
const WORKERS_ASSIGNMENT_DESCRIPTION = `Concurrent workers on the host (${MIN_WORKER_COUNT}–${MAX_WORKER_COUNT}).`;

// worker_count stays a STRING in the form (what a text field holds) and
// becomes a number only in `policyFromForm` — no coercion inside the schema,
// so the form's input and output types stay identical for react-hook-form.
const WORKER_COUNT_RANGE_MESSAGE = `Between ${MIN_WORKER_COUNT} and ${MAX_WORKER_COUNT} workers`;

export const policyFormSchema = z.object({
  sandbox_tier: z.enum(SANDBOX_TIERS),
  network_policy: z.enum(NETWORK_POLICIES),
  registry_allowlist: z.string().trim(),
  worker_count: z
    .string()
    .trim()
    .refine((v) => {
      const n = Number(v);
      return Number.isInteger(n) && n >= MIN_WORKER_COUNT && n <= MAX_WORKER_COUNT;
    }, WORKER_COUNT_RANGE_MESSAGE),
});
export type PolicyFormValues = z.infer<typeof policyFormSchema>;

export const POLICY_FORM_DEFAULTS: PolicyFormValues = {
  sandbox_tier: DEFAULT_ASSIGNED_SANDBOX_TIER,
  network_policy: DEFAULT_ASSIGNED_NETWORK_POLICY,
  registry_allowlist: "",
  worker_count: String(DEFAULT_WORKER_COUNT),
};

/** Form values → the wire assignment; surfaces the registry parse error. */
export function policyFromForm(values: PolicyFormValues): { policy: AssignedPolicy | null; error: string | null } {
  const parsed = parseRegistryAllowlist(values.registry_allowlist);
  if (parsed.error) return { policy: null, error: parsed.error };
  return {
    policy: {
      sandbox_tier: values.sandbox_tier,
      network_policy: values.network_policy,
      registry_allowlist: parsed.hosts,
      worker_count: Number(values.worker_count),
    },
    error: null,
  };
}

/** A stored assignment → form values, for the edit dialog's initial state. */
export function formFromPolicy(policy: AssignedPolicy): PolicyFormValues {
  return {
    sandbox_tier: policy.sandbox_tier,
    network_policy: policy.network_policy,
    registry_allowlist: policy.registry_allowlist.join(", "),
    worker_count: String(policy.worker_count),
  };
}

export function PolicyFields({ control }: { control: Control<PolicyFormValues> }) {
  const c = control;
  const isolationModeLabelId = useId();
  return (
    <>
      <FormField
        control={c}
        name="sandbox_tier"
        render={({ field }) => (
          <FormItem>
            {/* RadioGroup's root renders a <div role="radiogroup">, not a
                labelable HTML element — FormLabel's htmlFor (built for a
                single input/button/select) can't auto-focus it, so the
                group is named directly via aria-labelledby instead. */}
            <FormLabel id={isolationModeLabelId}>Isolation to assign</FormLabel>
            <FormControl>
              <RadioGroup
                value={field.value}
                onValueChange={field.onChange}
                aria-labelledby={isolationModeLabelId}
                className="sm:grid-cols-2"
              >
                {SANDBOX_TIERS.map((t) => (
                  <OptionCard
                    key={t}
                    value={t}
                    label={SANDBOX_TIER_LABELS[t]}
                    description={SANDBOX_TIER_DESCRIPTIONS[t]}
                  />
                ))}
              </RadioGroup>
            </FormControl>
            <FormDescription>{ISOLATION_ASSIGNMENT_DESCRIPTION}</FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={c}
        name="network_policy"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{NETWORK_ASSIGNMENT_LABEL}</FormLabel>
            <Select value={field.value} onValueChange={field.onChange}>
              <FormControl>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
              </FormControl>
              <SelectContent>
                {NETWORK_POLICIES.map((p) => (
                  <SelectItem key={p} value={p}>
                    {NETWORK_POLICY_LABELS[p]}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
            <FormDescription>{NETWORK_POLICY_DESCRIPTIONS[field.value]}</FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={c}
        name="registry_allowlist"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{REGISTRY_ASSIGNMENT_LABEL}</FormLabel>
            <FormControl>
              <Input placeholder="registry.npmjs.org, pypi.org" autoComplete="off" {...field} />
            </FormControl>
            <FormDescription>{REGISTRY_ASSIGNMENT_DESCRIPTION}</FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
      <FormField
        control={c}
        name="worker_count"
        render={({ field }) => (
          <FormItem>
            <FormLabel>{WORKERS_ASSIGNMENT_LABEL}</FormLabel>
            <FormControl>
              <Input
                type="number"
                inputMode="numeric"
                min={MIN_WORKER_COUNT}
                max={MAX_WORKER_COUNT}
                autoComplete="off"
                {...field}
              />
            </FormControl>
            <FormDescription>{WORKERS_ASSIGNMENT_DESCRIPTION}</FormDescription>
            <FormMessage />
          </FormItem>
        )}
      />
    </>
  );
}
