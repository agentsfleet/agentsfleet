"use client";

import { useRef, useState } from "react";
import type { Control } from "react-hook-form";
import {
  FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@agentsfleet/design-system";
import { listAdminModelsAction } from "../../models/actions";
import type { AdminModel } from "@/lib/api/admin_model_library";
import type { PolicyFormValues } from "./PolicyFields";

const NONE = "none";
type Catalogue = {kind: "idle" | "loading"} | {kind: "ready"; models: AdminModel[]} | {kind: "failed"};

/** Load the existing catalogue only when an operator opens this selection. */
export function PolicyDecisionField({control}: {control: Control<PolicyFormValues>}) {
  const [catalogue, setCatalogue] = useState<Catalogue>({kind: "idle"});
  const loading = useRef(false);
  async function load(open: boolean) {
    if (!open || loading.current || catalogue.kind === "ready") return;
    loading.current = true;
    setCatalogue({kind: "loading"});
    try {
      const result = await listAdminModelsAction();
      setCatalogue(result.ok ? {kind: "ready", models: result.data.models} : {kind: "failed"});
    } catch { setCatalogue({kind: "failed"}); }
    finally { loading.current = false; }
  }
  const models = catalogue.kind === "ready" ? catalogue.models.filter(model => model.decision?.enabled) : [];
  return <FormField control={control} name="decision_model_id" render={({field}) => {
    const current = field.value ?? "";
    const retained = current.length > 0 && !models.some(model => model.id === current);
    return <FormItem>
      <FormLabel>Decision model</FormLabel>
      <Select value={field.value || NONE} onValueChange={value => field.onChange(value === NONE ? "" : value)} onOpenChange={open => {void load(open);}}>
        <FormControl><SelectTrigger><SelectValue /></SelectTrigger></FormControl>
        <SelectContent>
          <SelectItem value={NONE}>None</SelectItem>
          {retained ? <SelectItem value={current} disabled>Current selection{catalogue.kind === "ready" ? " (unavailable)" : ""}</SelectItem> : null}
          {models.map(model => <SelectItem key={model.id} value={model.id}>{model.metadata.display_name} · {model.provider}</SelectItem>)}
        </SelectContent>
      </Select>
      <FormDescription>
        {catalogue.kind === "loading" ? "Loading reviewed decision models…" : catalogue.kind === "failed"
          ? "Could not load models. Reopen to retry; model read access is required. Your current selection is retained."
          : "Used for request routing and completion checks. Provider keys are managed through platform Secrets."}
      </FormDescription>
      <FormMessage />
    </FormItem>;
  }} />;
}
