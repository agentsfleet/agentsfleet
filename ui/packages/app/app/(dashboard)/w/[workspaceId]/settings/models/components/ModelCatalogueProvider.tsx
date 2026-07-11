"use client";

import { createContext, useContext, useEffect, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import type { LibraryModel } from "@/lib/api/model_library";
import { getModelLibraryAction } from "../actions";

// The model library rides a single client-side fetch on mount, through the
// token-minting Server Action (the GET /v1/models read is bearer-authed; the
// token never reaches the browser). Every picker reads it from context instead
// of props, so the library is fetched ONCE per session regardless of how many
// pickers mount. A 401 means the session, not the catalogue — the user routes
// to sign-in. Any other failure (network, 5xx) degrades pickers to free-text
// model entry (error=true).

export type ModelCatalogueState = {
  models: LibraryModel[];
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
  const router = useRouter();

  useEffect(() => {
    let active = true;
    getModelLibraryAction()
      .then((res) => {
        if (!active) return;
        if (res.ok) {
          setState({ models: res.data.models, loading: false, error: false });
          return;
        }
        // An expired session must not become a silent free-text degrade —
        // the user would hand-type model ids into a signed-out page.
        if (res.status === 401) {
          router.push("/sign-in");
          return;
        }
        setState(FALLBACK_STATE);
      })
      .catch(() => {
        if (active) setState(FALLBACK_STATE);
      });
    return () => {
      active = false;
    };
  }, [router]);

  return <ModelCatalogueContext.Provider value={state}>{children}</ModelCatalogueContext.Provider>;
}

/** Read the once-per-session catalogue. Returns a safe degraded state if no provider is mounted. */
export function useModelCatalogue(): ModelCatalogueState {
  return useContext(ModelCatalogueContext) ?? FALLBACK_STATE;
}
