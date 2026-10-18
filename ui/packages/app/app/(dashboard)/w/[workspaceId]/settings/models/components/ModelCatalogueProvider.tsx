"use client";

import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import type { LibraryModel } from "@/lib/api/model-library-types";
import { getModelLibraryAction } from "../actions";
import { CATALOGUE_STATUS, type CatalogueStatus } from "./catalogue-status";

export type ModelCatalogueState = {
  models: LibraryModel[];
  status: CatalogueStatus;
  preload: () => void;
  preloadAdvanced: () => void;
};

const FALLBACK_STATE: ModelCatalogueState = {
  models: [], status: CATALOGUE_STATUS.error, preload: () => {}, preloadAdvanced: () => {},
};
const ModelCatalogueContext = createContext<ModelCatalogueState | null>(null);

/** Authenticated catalogue reads start on intent; advanced entries require a request. */
export function ModelCatalogueProvider({ children }: { children: ReactNode }) {
  const state = useCatalogueState();
  return <ModelCatalogueContext.Provider value={state}>{children}</ModelCatalogueContext.Provider>;
}

function useCatalogueState(): ModelCatalogueState {
  const [models, setModels] = useState<LibraryModel[]>([]);
  const [status, setStatus] = useState<CatalogueStatus>(CATALOGUE_STATUS.idle);
  const [advancedLoaded, setAdvancedLoaded] = useState(false);
  const inFlight = useRef<boolean | null>(null);
  const pendingAdvanced = useRef(false);
  const router = useRouter();
  const load = useCallback(function load(advanced: boolean) {
    if (inFlight.current !== null) {
      if (advanced && !inFlight.current) pendingAdvanced.current = true;
      return;
    }
    if (status === CATALOGUE_STATUS.ready && (!advanced || advancedLoaded)) return;
    inFlight.current = advanced;
    setStatus(CATALOGUE_STATUS.loading);
    getModelLibraryAction(advanced).then((response) => {
      if (response.ok) {
        setModels(response.data.models);
        setAdvancedLoaded(advanced);
        setStatus(CATALOGUE_STATUS.ready);
      } else if (response.status === 401) router.push("/sign-in");
      else setStatus(CATALOGUE_STATUS.error);
    }).catch(() => {
      setStatus(CATALOGUE_STATUS.error);
    }).finally(() => {
      inFlight.current = null;
      // Serialize an advanced request made while the default read was in flight.
      if (pendingAdvanced.current) {
        pendingAdvanced.current = false;
        load(true);
      }
    });
  }, [advancedLoaded, router, status]);
  return { models, status, preload: () => load(false), preloadAdvanced: () => load(true) };
}

export function useModelCatalogue(): ModelCatalogueState {
  return useContext(ModelCatalogueContext) ?? FALLBACK_STATE;
}
