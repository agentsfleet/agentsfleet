"use client";

import { useState, useTransition } from "react";
import { useForm } from "react-hook-form";
import { zodResolver } from "@hookform/resolvers/zod";
import { z } from "zod";
import {
  ActionForm,
  Alert,
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
import { type AdminModel } from "@/lib/api/admin_model_library";
import { SETTINGS_DEFAULTS, settingsSchema, modelSettings } from "./model-settings";
import { ModelSettingsFields } from "./ModelSettingsFields";
import { presentErrorString } from "@/lib/errors";
import { createAdminModelAction } from "../actions";

const schema = settingsSchema.safeExtend({
  provider: z.string().trim().min(1, "Name the provider").max(64),
  model_id: z.string().trim().min(1, "Name the model").max(256),
});
type FormValues = z.infer<typeof schema>;
const DEFAULTS: FormValues = { ...SETTINGS_DEFAULTS, provider: "", model_id: "" };
const CREATE_MODEL_LIBRARY_TOOLTIP = "Create a priced model users can choose.";

export default function AddModelDialog({
  onCreated,
  defaultOpen = false,
}: {
  onCreated: (m: AdminModel) => void;
  defaultOpen?: boolean;
}) {
  const [open, setOpen] = useState(defaultOpen);
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
      try {
        const r = await createAdminModelAction({
          provider: v.provider,
          model_id: v.model_id,
          ...modelSettings(v),
        });
        if (!r.ok) {
          setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "create the model" }));
          return;
        }
        onCreated(r.data);
        handleOpenChange(false);
      } catch { setApiError("Could not create the model. Your entries are retained; try again."); }
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
      <DialogContent className="max-w-wide">
        <DialogHeader>
          <DialogTitle>Create model library</DialogTitle>
          <DialogDescription>
            Create a priced model users can choose. Prices are US dollars per 1M tokens.
          </DialogDescription>
        </DialogHeader>
        <Form {...form}>
          <ActionForm noValidate onSubmit={(e) => void form.handleSubmit(onSubmit)(e)} className="space-y-lg">
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
            <ModelSettingsFields />
            {apiError ? <Alert variant="destructive">{apiError}</Alert> : null}
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
                Create
              </TooltipButton>
            </DialogFooter>
          </ActionForm>
        </Form>
      </DialogContent>
    </Dialog>
  );
}
