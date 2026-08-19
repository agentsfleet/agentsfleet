"use client";

import { useState, useTransition } from "react";
import { PencilIcon } from "lucide-react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Form,
  Spinner,
} from "@agentsfleet/design-system";
import type { AssignedPolicy } from "@/lib/api/runners";
import { presentErrorString } from "@/lib/errors";
import { updateRunnerPolicyAction } from "../actions";
import {
  POLICY_FORM_DEFAULTS,
  PolicyFields,
  formFromPolicy,
  policyFormSchema,
  policyFromForm,
  type PolicyFormValues,
} from "./PolicyFields";

// Re-assign a runner's policy from its header — the dashboard IS the fix path
// for a degraded runner (there is no environment override to fall back to).
// The saved assignment reaches the host on its next heartbeat; no host visit,
// no restart. Reuses the enrollment dialog's four-field section verbatim.

export const EDIT_POLICY_LABEL = "Edit policy";
const EDIT_POLICY_DESCRIPTION =
  "Applied on the host's next heartbeat; growing workers past the daemon's start count needs a runner restart.";
const SAVE_POLICY_LABEL = "Save";
const EDIT_POLICY_ERROR_ACTION = "re-assign the policy";

export function EditPolicyDialog({
  runnerId,
  current,
  onSaved,
}: {
  runnerId: string;
  /** The stored assignment, or null for a pre-policy row (form starts at the defaults). */
  current: AssignedPolicy | null;
  onSaved: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const initial: PolicyFormValues = current ? formFromPolicy(current) : POLICY_FORM_DEFAULTS;
  const form = useForm<PolicyFormValues>({
    resolver: zodResolver(policyFormSchema),
    defaultValues: initial,
  });

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (!next) {
      setApiError(null);
      form.reset(current ? formFromPolicy(current) : POLICY_FORM_DEFAULTS);
    }
  }

  function onSubmit(values: PolicyFormValues) {
    setApiError(null);
    const assignment = policyFromForm(values);
    if (assignment.error !== null || assignment.policy === null) {
      setApiError(assignment.error);
      return;
    }
    const assigned_policy = assignment.policy;
    startTransition(async () => {
      const r = await updateRunnerPolicyAction(runnerId, assigned_policy);
      if (!r.ok) {
        setApiError(
          presentErrorString({ errorCode: r.errorCode, message: r.error, action: EDIT_POLICY_ERROR_ACTION }),
        );
        return;
      }
      setOpen(false);
      onSaved();
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button type="button" variant="outline" size="sm">
          <PencilIcon aria-hidden="true" /> {EDIT_POLICY_LABEL}
        </Button>
      </DialogTrigger>
      {/* The policy form is taller than a short viewport: three isolation cards,
          network policy, registry allowlist, workers, then the footer actions.
          Without a bounded height and its own scroll the Save button falls below
          the fold with no way to reach it, and assigning a policy — the only
          thing that makes a runner useful — needs a maximised window. */}
      <DialogContent className="max-h-svh overflow-y-auto">
        <DialogHeader>
          <DialogTitle>{EDIT_POLICY_LABEL}</DialogTitle>
          <DialogDescription>{EDIT_POLICY_DESCRIPTION}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form
            onSubmit={(e) => {
              void form.handleSubmit(onSubmit)(e);
            }}
            className="space-y-4"
          >
            <PolicyFields control={form.control} />
            {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
            <DialogFooter>
              <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={pending}>
                {pending ? <Spinner size="sm" srLabel="Saving assignment" /> : null}
                {SAVE_POLICY_LABEL}
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
