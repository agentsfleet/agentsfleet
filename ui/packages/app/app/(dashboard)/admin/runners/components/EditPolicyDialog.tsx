"use client";

import { useId, useState, useTransition } from "react";
import { PencilIcon } from "lucide-react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  Button,
  ActionForm,
  Alert,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Form,
  Input,
  Label,
  Spinner,
} from "@agentsfleet/design-system";
import { parseLabels, type AssignedPolicy } from "@/lib/api/runners-types";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { updateRunnerAction } from "../actions";
import { EDIT_POLICY_LABEL } from "./RunnerListCells";
import {
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

const EDIT_POLICY_TITLE = "Edit runner";
const EDIT_POLICY_DESCRIPTION =
  "Applied on the host's next heartbeat; growing workers past the daemon's start count needs a runner restart.";
const SAVE_POLICY_LABEL = "Save";
const EDIT_POLICY_ERROR_ACTION = "save the runner";

export function EditPolicyDialog({
  runnerId,
  current,
  currentLabels = [],
  onSaved,
}: {
  runnerId: string;
  /** The stored assignment the form starts from. */
  current: AssignedPolicy;
  currentLabels?: string[];
  onSaved: () => void;
}) {
  const labelsId = useId();
  const [labels, setLabels] = useState(currentLabels.join(", "));
  const [originalLabels, setOriginalLabels] = useState(labels);
  const [open, setOpen] = useState(false);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const initial: PolicyFormValues = formFromPolicy(current);
  const form = useForm<PolicyFormValues>({
    resolver: zodResolver(policyFormSchema),
    defaultValues: initial,
  });
  const policyChanged = form.formState.isDirty;

  function handleOpenChange(next: boolean) {
    if (pending) return;
    setOpen(next);
    setApiError(null);
    setLabels(currentLabels.join(", "));
    setOriginalLabels(currentLabels.join(", "));
    form.reset(formFromPolicy(current));
  }

  function onSubmit(values: PolicyFormValues) {
    setApiError(null);
    // Stored labels can contain commas or spaces; only parse an intentional edit.
    const parsedLabels = labels === originalLabels ? null : parseLabels(labels);
    if (parsedLabels !== null && parsedLabels.error !== null) {
      setApiError(parsedLabels.error);
      return;
    }
    const assignment = policyFromForm(values);
    if (assignment.error !== null || assignment.policy === null) {
      setApiError(assignment.error);
      return;
    }
    const assigned_policy = assignment.policy;
    const edit = parsedLabels === null
      ? { assigned_policy }
      : policyChanged
        ? { assigned_policy, labels: parsedLabels.labels }
        : { labels: parsedLabels.labels };
    startTransition(async () => {
      try {
      const r = await updateRunnerAction(runnerId, edit);
      if (!r.ok) {
        setApiError(
          presentErrorString({ errorCode: r.errorCode, message: r.error, action: EDIT_POLICY_ERROR_ACTION }),
        );
        return;
      }
      setOpen(false);
      onSaved();
      } catch (cause) {
        setApiError(unexpectedErrorMessage(cause, EDIT_POLICY_ERROR_ACTION));
      }
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <Button type="button" variant="ghost" size="sm">
          <PencilIcon aria-hidden="true" /> {EDIT_POLICY_LABEL}
        </Button>
      </DialogTrigger>
      {/* The policy form is taller than a short viewport: three isolation cards,
          network policy, registry allowlist, workers, then the footer actions.
          Without a bounded height and its own scroll the Save button falls below
          the fold with no way to reach it, and assigning a policy — the only
          thing that makes a runner useful — needs a maximised window. */}
      <DialogContent closeDisabled={pending}>
        <DialogHeader>
          <DialogTitle>{EDIT_POLICY_TITLE}</DialogTitle>
          <DialogDescription>{EDIT_POLICY_DESCRIPTION}</DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <ActionForm
            onSubmit={(e) => {
              void form.handleSubmit(onSubmit)(e);
            }}
            className="space-y-4"
          >
            <PolicyFields control={form.control} />
            <div className="space-y-2">
              <Label htmlFor={labelsId}>Labels</Label>
              <Input id={labelsId} value={labels} onChange={(event) => setLabels(event.target.value)} autoComplete="off" aria-describedby={`${labelsId}-help`} />
              <p id={`${labelsId}-help`} className="text-sm text-muted-foreground">Comma-separated placement labels. Clearing removes every label. Changes apply on the next lease placement.</p>
            </div>
            {apiError ? <Alert variant="destructive">{apiError}</Alert> : null}
            <DialogFooter>
              <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>
                Cancel
              </Button>
              <Button type="submit" disabled={pending}>
                {pending ? <Spinner size="sm" srLabel="Saving assignment" /> : null}
                {SAVE_POLICY_LABEL}
              </Button>
            </DialogFooter>
          </ActionForm>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
