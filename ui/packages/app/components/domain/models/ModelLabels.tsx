"use client";

import { createContext, useContext, type ReactNode } from "react";
import { NO_MODEL_LABELS, type ModelLabels } from "@/lib/model-labels";

const ModelLabelsContext = createContext<ModelLabels>(NO_MODEL_LABELS);

/** The dashboard's one source of model display names, supplied by the server render. */
export function ModelLabelsProvider({ labels, children }: { labels: ModelLabels; children: ReactNode }) {
  return <ModelLabelsContext.Provider value={labels}>{children}</ModelLabelsContext.Provider>;
}

/** The labels the layout read for this document; empty outside the provider. Name a model through `modelLabel`. */
export function useModelLabels(): ModelLabels {
  return useContext(ModelLabelsContext);
}
