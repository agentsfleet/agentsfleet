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
  DialogTrigger,
  Form,
  FormControl,
  FormDescription,
  FormField,
  FormItem,
  FormLabel,
  FormMessage,
  Input,
  Spinner,
  TooltipButton,
} from "@agentsfleet/design-system";
import { PlusIcon } from "lucide-react";
import { type AdminModel, usdPerMtokToNanos } from "@/lib/api/admin_models";
import { presentErrorString } from "@/lib/errors";
import { createAdminModelAction } from "../actions";

// Inputs are strings (react-hook-form controlled inputs) validated as numbers and
// converted at submit. Rates are entered in $/1M tokens (how providers quote) and
// turned into integer nanos; non-negative so a self-managed-only model prices 0.
const rate = z.string().trim().refine((s) => s !== "" && !Number.isNaN(Number(s)) && Number(s) >= 0, "must be a number >= 0");
const schema = z.object({
  provider: z.string().trim().min(1).max(64),
  model_id: z.string().trim().min(1).max(256),
  context_cap_tokens: z.string().trim().refine((s) => Number.isInteger(Number(s)) && Number(s) > 0, "must be a positive integer"),
  input_usd: rate,
  cached_usd: rate,
  output_usd: rate,
});
type FormValues = z.infer<typeof schema>;

const DEFAULTS: FormValues = {
  provider: "",
  model_id: "",
  context_cap_tokens: "128000",
  input_usd: "0",
  cached_usd: "0",
  output_usd: "0",
};
const CREATE_MODEL_LIBRARY_TOOLTIP = "Create a priced model users can choose.";

export default function AddModelDialog({ onCreated }: { onCreated: (m: AdminModel) => void }) {
  const [open, setOpen] = useState(false);
  const [apiError, setApiError] = useState<string | null>(null);
  const [pending, startTransition] = useTransition();
  const form = useForm<FormValues>({ resolver: zodResolver(schema), defaultValues: DEFAULTS });

  function handleOpenChange(next: boolean) {
    setOpen(next);
    if (!next) {
      setApiError(null);
      form.reset(DEFAULTS);
    }
  }

  function onSubmit(values: FormValues) {
    setApiError(null);
    const v = schema.parse(values);
    startTransition(async () => {
      const r = await createAdminModelAction({
        provider: v.provider,
        model_id: v.model_id,
        context_cap_tokens: Number(v.context_cap_tokens),
        input_nanos_per_mtok: usdPerMtokToNanos(Number(v.input_usd)),
        cached_input_nanos_per_mtok: usdPerMtokToNanos(Number(v.cached_usd)),
        output_nanos_per_mtok: usdPerMtokToNanos(Number(v.output_usd)),
      });
      if (!r.ok) {
        setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "create the model" }));
        return;
      }
      onCreated(r.data);
      handleOpenChange(false);
    });
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogTrigger asChild>
        <TooltipButton type="button" size="sm" tooltip={CREATE_MODEL_LIBRARY_TOOLTIP}>
          <PlusIcon size={14} />
          Create model library
        </TooltipButton>
      </DialogTrigger>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Create model library</DialogTitle>
          <DialogDescription>
            Create a priced model users can choose. Rates are per 1M tokens.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <form onSubmit={(e) => void form.handleSubmit(onSubmit)(e)} className="space-y-4">
            <FormField control={form.control} name="provider" render={({ field }) => (
              <FormItem>
                <FormLabel>Provider</FormLabel>
                <FormControl><Input placeholder="fireworks" autoComplete="off" {...field} /></FormControl>
                <FormMessage />
              </FormItem>
            )} />
            <FormField control={form.control} name="model_id" render={({ field }) => (
              <FormItem>
                <FormLabel>Model</FormLabel>
                <FormControl><Input placeholder="glm-5.2" autoComplete="off" className="font-mono" {...field} /></FormControl>
                <FormDescription>The provider&apos;s model identifier (may contain slashes).</FormDescription>
                <FormMessage />
              </FormItem>
            )} />
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
              <Button
                type="button"
                variant="ghost"
                disabled={pending}
                onClick={() => handleOpenChange(false)}
              >
                Cancel
              </Button>
              <TooltipButton type="submit" disabled={pending} tooltip={CREATE_MODEL_LIBRARY_TOOLTIP}>
                {pending ? <Spinner size="sm" srLabel="Creating" /> : null}
                Create model library
              </TooltipButton>
            </DialogFooter>
          </form>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
