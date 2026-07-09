"use client";

import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  Button,
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
  Form,
  FormControl,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
} from "@agentsfleet/design-system";
import {
  type AdminModel,
  type ModelRatesInput,
  nanosToUsdPerMtok,
  usdPerMtokToNanos,
} from "@/lib/api/admin_models";
import { presentErrorString } from "@/lib/errors";
import { updateAdminModelAction } from "../actions";

// Same rate/cap shape as AddModelDialog, minus provider/model_id — those are the
// row's immutable identity (a PATCH edits caps/rates only). Inputs are strings
// (react-hook-form controlled) validated as numbers and converted at submit.
const rate = z.string().trim().refine((s) => s !== "" && !Number.isNaN(Number(s)) && Number(s) >= 0, "must be a number >= 0");
const schema = z.object({
  context_cap_tokens: z.string().trim().refine((s) => Number.isInteger(Number(s)) && Number(s) > 0, "must be a positive integer"),
  input_usd: rate,
  cached_usd: rate,
  output_usd: rate,
});
type FormValues = z.infer<typeof schema>;

// Nanos → $/1M for the pre-fill (the form edits in $/1M, how providers quote).
function valuesFromModel(m: AdminModel): FormValues {
  return {
    context_cap_tokens: String(m.context_cap_tokens),
    input_usd: String(nanosToUsdPerMtok(m.input_nanos_per_mtok)),
    cached_usd: String(nanosToUsdPerMtok(m.cached_input_nanos_per_mtok)),
    output_usd: String(nanosToUsdPerMtok(m.output_nanos_per_mtok)),
  };
}

// Mounted only while a row is being edited (parent keys it by uid), so the form
// initialises pre-filled from that row — no reset-on-open effect needed. The
// dialog is always open when mounted; onOpenChange(false) tells the parent to
// unmount.
export default function EditModelDialog({
  model,
  onOpenChange,
  onUpdated,
}: {
  model: AdminModel;
  onOpenChange: (open: boolean) => void;
  onUpdated: (m: AdminModel) => void;
}) {
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: valuesFromModel(model) });

  function onSubmit(values: FormValues) {
    setApiError(null);
    const v = schema.parse(values);
    const rates: ModelRatesInput = {
      context_cap_tokens: Number(v.context_cap_tokens),
      input_nanos_per_mtok: usdPerMtokToNanos(Number(v.input_usd)),
      cached_input_nanos_per_mtok: usdPerMtokToNanos(Number(v.cached_usd)),
      output_nanos_per_mtok: usdPerMtokToNanos(Number(v.output_usd)),
    };
    startTransition(async () => {
      const r = await updateAdminModelAction(model.uid, rates);
      if (!r.ok) {
        setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "update the model" }));
        return;
      }
      // The PATCH returns {uid, updated}; the row's new shape is its identity plus
      // the edited rates — hand that to the parent so the table updates in place.
      onUpdated({ ...model, ...rates });
      onOpenChange(false);
    });
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit model rates</DialogTitle>
          <DialogDescription>
            Update the context cap and per-token rates. Provider and model id are the row&apos;s fixed
            identity and can&apos;t change here. Rates are per 1M tokens.
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-3">
          <div className="space-y-1.5">
            <label htmlFor="edit-provider" className="text-sm font-medium">Provider</label>
            <Input id="edit-provider" value={model.provider} disabled />
          </div>
          <div className="space-y-1.5">
            <label htmlFor="edit-model-id" className="text-sm font-medium">Model id</label>
            <Input id="edit-model-id" value={model.model_id} disabled />
          </div>
        </div>
        <Form {...form}>
          <form onSubmit={(e) => void form.handleSubmit(onSubmit)(e)} className="space-y-4">
            <FormField control={form.control} name="context_cap_tokens" render={({ field }) => (
              <FormItem>
                <FormLabel>Context cap (tokens)</FormLabel>
                <FormControl><Input type="number" min={1} className="font-mono" {...field} /></FormControl>
                <FormMessage />
              </FormItem>
            )} />
            <div className="grid grid-cols-3 gap-3">
              <FormField control={form.control} name="input_usd" render={({ field }) => (
                <FormItem>
                  <FormLabel>Input $/1M</FormLabel>
                  <FormControl><Input type="number" min={0} step="0.01" className="font-mono" {...field} /></FormControl>
                  <FormMessage />
                </FormItem>
              )} />
              <FormField control={form.control} name="cached_usd" render={({ field }) => (
                <FormItem>
                  <FormLabel>Cached $/1M</FormLabel>
                  <FormControl><Input type="number" min={0} step="0.01" className="font-mono" {...field} /></FormControl>
                  <FormMessage />
                </FormItem>
              )} />
              <FormField control={form.control} name="output_usd" render={({ field }) => (
                <FormItem>
                  <FormLabel>Output $/1M</FormLabel>
                  <FormControl><Input type="number" min={0} step="0.01" className="font-mono" {...field} /></FormControl>
                  <FormMessage />
                </FormItem>
              )} />
            </div>
            {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
            <DialogFooter>
              <Button type="submit" disabled={pending}>
                {pending ? <Spinner size="sm" srLabel="Saving" /> : null}
                Save changes
              </Button>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
