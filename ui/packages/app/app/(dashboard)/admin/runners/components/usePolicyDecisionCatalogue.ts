"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import { useFormContext } from "react-hook-form";
import type { AdminModel } from "@/lib/api/admin_model_library";
import { listAdminModelsAction } from "../../models/actions";
import type { PolicyFormValues } from "./PolicyFields";

export type DecisionModel = AdminModel & {decision: NonNullable<AdminModel["decision"]>};
type Catalogue = {kind: "idle" | "loading"} | {kind: "ready"; models: DecisionModel[]} | {kind: "failed"};

export function usePolicyDecisionCatalogue(onPendingChange: (pending: boolean) => void) {
  const { getValues, resetField } = useFormContext<PolicyFormValues>();
  const [catalogue, setCatalogue] = useState<Catalogue>({kind: "idle"});
  const loading = useRef(false);
  const ready = useRef(false);
  const mounted = useRef(false);
  const load = useCallback(async () => {
    if (loading.current || ready.current) return;
    loading.current = true;
    onPendingChange(getValues("decision_model_id") === undefined);
    setCatalogue({kind: "loading"});
    try {
      const result = await listAdminModelsAction();
      if (!mounted.current) return;
      if (!result.ok) throw new Error("Decision catalogue unavailable");
      const models = result.data.models.filter((model): model is DecisionModel => model.decision?.enabled === true);
      const jev = models.find(model => model.model_id === "jev-latest" && model.decision.profile.api === "typesafe");
      if (getValues("decision_model_id") === undefined && jev) {
        resetField("decision_model_id", {defaultValue: jev.id});
      }
      ready.current = true;
      setCatalogue({kind: "ready", models});
      onPendingChange(getValues("decision_model_id") === undefined);
    } catch {
      if (mounted.current) {
        setCatalogue({kind: "failed"});
        onPendingChange(getValues("decision_model_id") === undefined);
      }
    } finally { loading.current = false; }
  }, [getValues, resetField, onPendingChange]);
  useEffect(() => {
    mounted.current = true;
    void load();
    return () => { mounted.current = false; };
  }, [load, onPendingChange]);
  return {catalogue, load};
}
