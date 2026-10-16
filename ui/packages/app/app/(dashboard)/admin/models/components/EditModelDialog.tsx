"use client";

import { useState, useTransition } from "react";
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
  Form,
  Input,
  Spinner,
} from "@agentsfleet/design-system";
import { type AdminModel, type ModelRatesInput } from "@/lib/api/admin_model_library";
import { settingsSchema as schema, modelSettings, valuesFromModel, type ModelSettingsValues } from "./model-settings";
import { ModelSettingsFields } from "./ModelSettingsFields";
import { presentErrorString } from "@/lib/errors";
import { updateAdminModelAction } from "../actions";

// Mounted only while a row is being edited (parent keys it by id), so the form
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
  const form = useForm<ModelSettingsValues>({ resolver: zodResolver(schema), defaultValues: valuesFromModel(model) });

  function onSubmit(values: ModelSettingsValues) {
    setApiError(null);
    const v = schema.parse(values);
    const rates: ModelRatesInput = modelSettings(v);
    startTransition(async () => {
      const r = await updateAdminModelAction(model.id, rates);
      if (!r.ok) {
        setApiError(presentErrorString({ errorCode: r.errorCode, message: r.error, action: "update the model" }));
        return;
      }
      // The PATCH returns {id, updated}; the row's new shape is its identity plus
      // the edited rates — hand that to the parent so the table updates in place.
      onUpdated({ ...model, ...rates });
      onOpenChange(false);
    });
  }

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit model</DialogTitle>
          <DialogDescription>
            Configure this model&apos;s inference API, endpoint, token limits and rates.
            Credentials stay in the workspace vault. Provider and model identify this row.
          </DialogDescription>
        </DialogHeader>
        <div className="grid grid-cols-2 gap-3">
          {([["Provider", model.provider], ["Model", model.model_id]] as const).map(([label, value]) => (
            <div key={label} className="space-y-1.5">
              <span className="text-sm font-medium">{label}</span>
              <Input value={value} disabled aria-label={`${label} (locked)`} />
            </div>
          ))}
        </div>
        <Form {...form}>
          <form onSubmit={(e) => void form.handleSubmit(onSubmit)(e)} className="space-y-4">
            <ModelSettingsFields />
            {apiError ? <p className="text-sm text-destructive">{apiError}</p> : null}
            <DialogFooter className="flex-col gap-2 sm:flex-row sm:gap-2">
              <Button type="button" variant="ghost" disabled={pending} onClick={() => onOpenChange(false)}>
                Cancel
              </Button>
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
