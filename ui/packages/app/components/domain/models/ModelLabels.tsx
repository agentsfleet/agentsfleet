"use client";

import { createContext, use, useContext, useMemo, type ReactNode } from "react";
import { NO_MODEL_LABELS, type ModelLabels } from "@/lib/model-labels";

type Store = {labels: ModelLabels; pending?: Promise<ModelLabels>};
const ModelLabelsContext = createContext<Store>({labels: NO_MODEL_LABELS});

/** The dashboard's one source of model display names, supplied by the server render. */
export function ModelLabelsProvider({ labels, pending, children }: Store & {children: ReactNode}) {
  const store = useMemo(() => ({labels, pending}), [labels, pending]);
  return <ModelLabelsContext.Provider value={store}>{children}</ModelLabelsContext.Provider>;
}

/** The labels the layout read for this document; empty outside the provider. Name a model through `modelLabel`. */
export function useModelLabels(): ModelLabels {
  const {labels, pending} = useContext(ModelLabelsContext);
  return pending ? use(pending) : labels;
}
