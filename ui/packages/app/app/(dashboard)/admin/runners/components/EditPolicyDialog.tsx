"use client";

import { useId, useState, useTransition } from "react";
import { CheckIcon, Clock3Icon, PencilIcon } from "lucide-react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import {
  Button, ActionForm, Alert, Dialog, DialogContent, DialogDescription,
  DialogFooter, DialogHeader, DialogTitle, DialogTrigger, Form, Input, Label, Spinner,
} from "@agentsfleet/design-system";
import { parseLabels, type AssignedPolicy } from "@/lib/api/runners-types";
import { presentErrorString, unexpectedErrorMessage } from "@/lib/errors";
import { updateRunnerAction } from "../actions";
import { EDIT_POLICY_LABEL } from "./RunnerListCells";
import { PolicyFields, formFromPolicy, policyFormSchema, policyFromForm, type PolicyFormValues } from "./PolicyFields";

type EditorProps = {
  runnerId: string;
  current: AssignedPolicy;
  currentLabels?: string[];
  onSaved: () => void;
};
const ERROR_ACTION = "save the runner";

function prepareEdit(values: PolicyFormValues, labels: string, originalLabels: string, policyChanged: boolean) {
  // Stored labels can contain commas or spaces; only parse an intentional edit.
  const parsedLabels = labels === originalLabels ? null : parseLabels(labels);
  if (parsedLabels?.error) return { error: parsedLabels.error, edit: null };
  const assignment = policyFromForm(values);
  if (assignment.error !== null || assignment.policy === null) return { error: assignment.error, edit: null };
  const edit = parsedLabels === null
    ? { assigned_policy: assignment.policy }
    : policyChanged
      ? { assigned_policy: assignment.policy, labels: parsedLabels.labels }
      : { labels: parsedLabels.labels };
  return { error: null, edit };
}

function usePolicyEditor({ runnerId, current, currentLabels = [], onSaved }: EditorProps) {
  const [labels, setLabels] = useState(currentLabels.join(", "));
  const [originalLabels, setOriginalLabels] = useState(labels);
  const [open, setOpen] = useState(false);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<PolicyFormValues>({ resolver: zodResolver(policyFormSchema), defaultValues: formFromPolicy(current) });
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
    const prepared = prepareEdit(values, labels, originalLabels, policyChanged);
    setApiError(prepared.error);
    if (!prepared.edit) return;
    const edit = prepared.edit;
    startTransition(async () => {
      try {
        const result = await updateRunnerAction(runnerId, edit);
        if (!result.ok) {
          setApiError(presentErrorString({ errorCode: result.errorCode, message: result.error, action: ERROR_ACTION }));
          return;
        }
        setOpen(false);
        onSaved();
      } catch (cause) {
        setApiError(unexpectedErrorMessage(cause, ERROR_ACTION));
      }
    });
  }
  return { open, handleOpenChange, form, labels, setLabels, apiError, pending, onSubmit };
}

type EditorState = ReturnType<typeof usePolicyEditor>;

export function EditPolicyDialog(props: EditorProps) {
  const editor = usePolicyEditor(props);
  return <Dialog open={editor.open} onOpenChange={editor.handleOpenChange}>
    <DialogTrigger asChild>
      <Button type="button" variant="ghost" size="sm">
        <PencilIcon aria-hidden="true" /> {EDIT_POLICY_LABEL}
      </Button>
    </DialogTrigger>
    <DialogContent closeDisabled={editor.pending}
      className="bottom-0 left-auto right-0 top-0 flex h-dvh max-h-dvh w-full max-w-none translate-x-0 translate-y-0 flex-col gap-0 overflow-hidden rounded-none border-y-0 border-r-0 p-0 data-[state=open]:slide-in-from-right-4 data-[state=closed]:slide-out-to-right-4 sm:max-w-form">
      <DialogHeader className="shrink-0 gap-sm border-b border-border px-xl py-xl pr-5xl sm:px-2xl sm:pr-5xl">
        <DialogTitle>Edit runner</DialogTitle>
        <DialogDescription>Configure how this runner handles fleet work.</DialogDescription>
      </DialogHeader>
      <RunnerPolicyForm editor={editor} />
    </DialogContent>
  </Dialog>;
}

function RunnerPolicyForm({ editor }: { editor: EditorState }) {
  const { form, labels, setLabels, apiError, pending, onSubmit, handleOpenChange } = editor;
  const [decisionPending, setDecisionPending] = useState(() => form.getValues("decision_model_id") === undefined);
  return <Form {...form}>
    <ActionForm noValidate onSubmit={event => {
      if (pending || decisionPending) { event.preventDefault(); return; }
      void form.handleSubmit(onSubmit)(event);
    }}
      className="flex min-h-0 flex-1 flex-col space-y-0">
      <fieldset disabled={pending} className="min-h-0 flex-1 overflow-y-auto overscroll-contain px-xl py-xl sm:px-2xl">
        <PolicyFields control={form.control} onDecisionPendingChange={setDecisionPending} />
        <RunnerLabels value={labels} onChange={setLabels} />
      </fieldset>
      <div className="shrink-0 border-t border-border bg-card px-xl py-lg sm:px-2xl">
        {apiError ? <Alert variant="destructive" className="mb-lg">{apiError}</Alert> : null}
        <DialogFooter className="items-center justify-between gap-lg pt-0">
          <p className="flex items-center gap-sm text-label text-muted-foreground">
            <Clock3Icon size={14} className="text-info" aria-hidden="true" /> Applies on the next heartbeat
          </p>
          <div className="flex items-center gap-sm">
            <Button type="button" variant="ghost" disabled={pending} onClick={() => handleOpenChange(false)}>Cancel</Button>
            <Button type="submit" disabled={pending || decisionPending}>
              {pending ? <Spinner size="sm" srLabel="Saving assignment" /> : <CheckIcon aria-hidden="true" />}
              Save
            </Button>
          </div>
        </DialogFooter>
      </div>
    </ActionForm>
  </Form>;
}

function RunnerLabels({ value, onChange }: { value: string; onChange: (value: string) => void }) {
  const labelsId = useId();
  return <div className="space-y-sm pb-sm pt-lg">
    <Label htmlFor={labelsId}>Labels</Label>
    <Input id={labelsId} value={value} onChange={event => onChange(event.target.value)}
      autoComplete="off" aria-describedby={`${labelsId}-help`} placeholder="linux, eu-west, gpu" />
    <p id={`${labelsId}-help`} className="text-body-sm text-muted-foreground">
      Comma-separated placement labels. Clearing removes every label. Changes apply on the next lease placement.
    </p>
  </div>;
}
