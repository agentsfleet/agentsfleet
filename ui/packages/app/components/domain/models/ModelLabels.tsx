"use client";

import { createContext, use, useContext, useMemo, type ReactNode } from "react";
import { NO_MODEL_LABELS, type ModelLabels } from "@/lib/model-labels";

type Store = {labels: ModelLabels; pending?: Promise<ModelLabels>};
const ModelLabelsContext = createContext<Store>({labels: NO_MODEL_LABELS});

/** Model display names for the billing report, supplied by its route layout. */
export function ModelLabelsProvider({ labels, pending, children }: Store & {children: ReactNode}) {
  const store = useMemo(() => ({labels, pending}), [labels, pending]);
  return <ModelLabelsContext.Provider value={store}>{children}</ModelLabelsContext.Provider>;
}

/** The billing table consumes the deferred labels inside its own Suspense boundary. */
export function useModelLabels(): ModelLabels {
  const {labels, pending} = useContext(ModelLabelsContext);
  return pending ? use(pending) : labels;
}
