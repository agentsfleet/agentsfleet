"use client";

import { z } from "zod";
import type { Control } from "react-hook-form";
import { Globe2Icon, LayersIcon, ListRestartIcon, SlidersHorizontalIcon } from "lucide-react";
import {
  DEFAULT_ASSIGNED_NETWORK_POLICY,
  DEFAULT_DURABLE_TASKS_ENABLED,
  DEFAULT_WORKER_COUNT,
  MAX_WORKER_COUNT,
  MIN_WORKER_COUNT,
  NETWORK_POLICIES,
  SANDBOX_TIERS,
  parseRegistryAllowlist,
  type AssignedPolicy,
  type SandboxTier,
} from "@/lib/api/runners-types";
import {
  BIND_MODES,
  MAX_EXTRA_BINDS,
  bindNoteIssue,
  bindPathIssue,
  bindsFromForm,
  formFromBinds,
} from "./policy-binds";
import { PolicyBindsField } from "./PolicyBindsField";
import { PolicyDecisionField } from "./PolicyDecisionField";
import { PolicyDurableTasksField } from "./PolicyDurableTasksField";

import { PolicyIsolationField } from "./PolicyIsolationField";
import { PolicyNetworkFields } from "./PolicyNetworkFields";
import { PolicySection } from "./PolicySection";
import { PolicyWorkerField } from "./PolicyWorkerField";
import { PolicyEngineField } from "./PolicyEngineField";

// PATCH replaces the whole assignment, so enrollment and editing share the
// schema and conversion. Every field survives an edit to another setting.
export const DEFAULT_ASSIGNED_SANDBOX_TIER: SandboxTier = "landlock_full";
// worker_count stays a STRING in the form (what a text field holds) and
// becomes a number only in `policyFromForm` — no coercion inside the schema,
// so the form's input and output types stay identical for react-hook-form.
const WORKER_COUNT_RANGE_MESSAGE = `Between ${MIN_WORKER_COUNT} and ${MAX_WORKER_COUNT} workers`;

export const policyFormSchema = z.object({
  durable_tasks_enabled: z.boolean(),
  decision_model_id: z.union([z.uuidv7(), z.literal("")]).optional(),
  sandbox_tier: z.enum(SANDBOX_TIERS),
  network_policy: z.enum(NETWORK_POLICIES),
  registry_allowlist: z.string().trim(),
  worker_count: z
    .string()
    .trim()
    .refine((v) => {
      const n = Number(v);
      return (
        Number.isInteger(n) && n >= MIN_WORKER_COUNT && n <= MAX_WORKER_COUNT
      );
    }, WORKER_COUNT_RANGE_MESSAGE),
  // A blank row is dropped on save rather than refused — an operator who opens
  // a row and changes their mind should still be able to save. Every filled row
  // is graded against the same rules `protocol_bind.extraBindsValid` applies.
  extra_binds: z
    .array(
      z.object({
        path: z.string(),
        mode: z.enum(BIND_MODES),
        note: z.string(),
      }),
    )
    .superRefine((rows, ctx) => {
      const filled = rows.filter((r) => r.path.trim().length > 0);
      if (filled.length > MAX_EXTRA_BINDS) {
        ctx.addIssue({
          code: "custom",
          message: `At most ${MAX_EXTRA_BINDS} mounts per runner`,
        });
      }
      rows.forEach((r, i) => {
        const path = r.path.trim();
        if (path.length === 0) return;
        const pathIssue = bindPathIssue(path);
        if (pathIssue)
          ctx.addIssue({
            code: "custom",
            message: pathIssue,
            path: [i, "path"],
          });
        const noteIssue = bindNoteIssue(r.note.trim());
        if (noteIssue)
          ctx.addIssue({
            code: "custom",
            message: noteIssue,
            path: [i, "note"],
          });
      });
    }),
});
export type PolicyFormValues = z.infer<typeof policyFormSchema>;

export const POLICY_FORM_DEFAULTS: PolicyFormValues = {
  durable_tasks_enabled: DEFAULT_DURABLE_TASKS_ENABLED,
  decision_model_id: undefined,
  sandbox_tier: DEFAULT_ASSIGNED_SANDBOX_TIER,
  network_policy: DEFAULT_ASSIGNED_NETWORK_POLICY,
  registry_allowlist: "",
  worker_count: String(DEFAULT_WORKER_COUNT),
  extra_binds: [],
};

/** Form values → the wire assignment; surfaces the registry parse error. */
export function policyFromForm(values: PolicyFormValues): {
  policy: AssignedPolicy | null;
  error: string | null;
} {
  const parsed = parseRegistryAllowlist(values.registry_allowlist);
  if (parsed.error) return { policy: null, error: parsed.error };
  return {
    policy: {
      durable_tasks_enabled: values.durable_tasks_enabled,
      ...(values.decision_model_id ? {decision_model_id: values.decision_model_id} : {}),
      sandbox_tier: values.sandbox_tier,
      network_policy: values.network_policy,
      registry_allowlist: parsed.hosts,
      worker_count: Number(values.worker_count),
      extra_binds: bindsFromForm(values.extra_binds),
    },
    error: null,
  };
}

/** A stored assignment → form values, for the edit dialog's initial state. */
export function formFromPolicy(policy: AssignedPolicy): PolicyFormValues {
  return {
    durable_tasks_enabled: policy.durable_tasks_enabled,
    decision_model_id: policy.decision_model_id ?? undefined,
    sandbox_tier: policy.sandbox_tier,
    network_policy: policy.network_policy,
    registry_allowlist: policy.registry_allowlist.join(", "),
    worker_count: String(policy.worker_count),
    extra_binds: formFromBinds(policy.extra_binds),
  };
}

export function PolicyFields({ control, onDecisionPendingChange }: {
  control: Control<PolicyFormValues>;
  onDecisionPendingChange: (pending: boolean) => void;
}) {
  return <div className="divide-y divide-border">
    <PolicySection title="Execution" icon={<LayersIcon size={14} className="text-info" aria-hidden="true" />}>
      <PolicyEngineField />
      <PolicyIsolationField control={control} />
      <PolicyWorkerField control={control} />
    </PolicySection>
    <PolicySection title="Network access" icon={<Globe2Icon size={14} className="text-pulse" aria-hidden="true" />}>
      <PolicyNetworkFields control={control} />
    </PolicySection>
    <PolicySection title="Durable work" icon={<ListRestartIcon size={14} className="text-success" aria-hidden="true" />}>
      <PolicyDurableTasksField control={control} />
      <PolicyDecisionField control={control} onPendingChange={onDecisionPendingChange} />
    </PolicySection>
    <PolicySection title="Advanced" icon={<SlidersHorizontalIcon size={14} className="text-evidence" aria-hidden="true" />}>
      <PolicyBindsField control={control} />
    </PolicySection>
  </div>;
}
