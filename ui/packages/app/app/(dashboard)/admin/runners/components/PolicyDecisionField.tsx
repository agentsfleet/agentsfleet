"use client";

import type { Control } from "react-hook-form";
import {
  FormControl, FormDescription, FormField, FormItem, FormLabel, FormMessage,
  Select, SelectContent, SelectItem, SelectTrigger, SelectValue,
} from "@agentsfleet/design-system";
import type { PolicyFormValues } from "./PolicyFields";
import { usePolicyDecisionCatalogue, type DecisionModel } from "./usePolicyDecisionCatalogue";

const DEFAULT = "default";
const NONE = "none";

function modelLabel(model: DecisionModel) {
  return model.model_id === "jev-latest" && model.decision.profile.api === "typesafe"
    ? "TypeSafe Jev"
    : `${model.metadata.display_name} · ${model.provider}`;
}

export function PolicyDecisionField({control, onPendingChange}: {
  control: Control<PolicyFormValues>;
  onPendingChange: (pending: boolean) => void;
}) {
  const {catalogue, load} = usePolicyDecisionCatalogue(onPendingChange);
  const models = catalogue.kind === "ready" ? catalogue.models : [];
  return <FormField control={control} name="decision_model_id" render={({field}) => {
    const current = field.value ?? "";
    const unselected = field.value === undefined;
    const retained = current.length > 0 && !models.some(model => model.id === current);
    return <FormItem>
      <FormLabel>Decision model</FormLabel>
      <Select value={unselected ? DEFAULT : current || NONE} onValueChange={value => {field.onChange(value === NONE ? "" : value); onPendingChange(false);}}
        onOpenChange={open => {if (open) void load();}}>
        <FormControl><SelectTrigger><SelectValue /></SelectTrigger></FormControl>
        <SelectContent>
          {unselected ? <SelectItem value={DEFAULT} disabled>TypeSafe Jev · {catalogue.kind === "loading" ? "loading…" : "setup required"}</SelectItem> : null}
          <SelectItem value={NONE}>None</SelectItem>
          {retained ? <SelectItem value={current} disabled>Current selection{catalogue.kind === "ready" ? " (unavailable)" : ""}</SelectItem> : null}
          {models.map(model => <SelectItem key={model.id} value={model.id}>{modelLabel(model)}</SelectItem>)}
        </SelectContent>
      </Select>
      <FormDescription>
        {catalogue.kind === "loading" ? "Loading reviewed decision models…" : catalogue.kind === "failed"
          ? "Could not load models. Reopen to retry; model read access is required. Your stored selection is retained. Choose None to continue without a decision model."
          : unselected
            ? "TypeSafe Jev needs setup. Enable its reviewed model in Models and add the typesafe secret in the platform-admin workspace."
            : current ? "Checks requests and goal progress. TypeSafe Jev uses the platform admin’s typesafe secret; provider keys stay outside this form."
              : "No decision model assigned. Select TypeSafe Jev after its platform setup is complete."}
      </FormDescription>
      <FormMessage />
    </FormItem>;
  }} />;
}
