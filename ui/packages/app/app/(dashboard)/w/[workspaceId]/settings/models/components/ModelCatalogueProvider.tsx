"use client";

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { getModelCaps, type ModelCap } from "@/lib/api/model_caps";

// The model catalogue (cap.json) used to ride the Models page's RSC payload —
// every server render paid a catalogue round-trip and shipped the whole list in
// the HTML. It changes only when the DB model rows change, so it moves here: a
// single client-side fetch on mount via getModelCaps(), which in the browser
// hits the same-origin `/backend` proxy (BASE) — no CORS, no Bearer token (the
// catalogue is public). Every picker reads it from context instead of props, so
// the catalogue is fetched ONCE per session regardless of how many pickers
// mount. A failed fetch degrades pickers to free-text model entry (error=true).

export type ModelCatalogueState = {
  models: ModelCap[];
  loading: boolean;
  error: boolean;
};

const INITIAL_STATE: ModelCatalogueState = { models: [], loading: true, error: false };

// Consumers rendered outside a provider degrade to free-text entry rather than
// throwing — the catalogue is an enhancement, never a hard dependency.
const FALLBACK_STATE: ModelCatalogueState = { models: [], loading: false, error: true };

const ModelCatalogueContext = createContext<ModelCatalogueState | null>(null);

export function ModelCatalogueProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<ModelCatalogueState>(INITIAL_STATE);

  useEffect(() => {
    let active = true;
    getModelCaps()
      .then((caps) => {
        if (active) setState({ models: caps.models, loading: false, error: false });
      })
      .catch(() => {
        if (active) setState(FALLBACK_STATE);
      });
    return () => {
      active = false;
    };
  }, []);

  return <ModelCatalogueContext.Provider value={state}>{children}</ModelCatalogueContext.Provider>;
}

/** Read the once-per-session catalogue. Returns a safe degraded state if no provider is mounted. */
export function useModelCatalogue(): ModelCatalogueState {
  return useContext(ModelCatalogueContext) ?? FALLBACK_STATE;
}
