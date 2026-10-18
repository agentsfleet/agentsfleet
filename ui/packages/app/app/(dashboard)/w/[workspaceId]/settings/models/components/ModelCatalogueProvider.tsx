"use client";

import { createContext, useCallback, useContext, useRef, useState,
  type Dispatch, type ReactNode, type RefObject, type SetStateAction } from "react";
import { useRouter } from "next/navigation";
import type { LibraryModel } from "@/lib/api/model-library-types";
import { getModelLibraryAction } from "../actions";
import { CATALOGUE_STATUS, type CatalogueStatus } from "./catalogue-status";

type PageQuery = { advanced: boolean; cursor: string | null };
type CatalogueData = {
  models: LibraryModel[]; status: CatalogueStatus; error: string | null;
  nextCursor: string | null; advancedNextCursor: string | null;
};
export type ModelCatalogueState = CatalogueData & {
  preload: () => void; preloadAdvanced: () => void;
  loadMore: (includeAdvanced?: boolean) => void; retry: () => void;
};
const INITIAL_DATA: CatalogueData = {
  models: [], status: CATALOGUE_STATUS.idle, error: null, nextCursor: null, advancedNextCursor: null,
};
const FALLBACK_STATE: ModelCatalogueState = {
  ...INITIAL_DATA, status: CATALOGUE_STATUS.error,
  preload: () => {}, preloadAdvanced: () => {}, loadMore: () => {}, retry: () => {},
};
const ModelCatalogueContext = createContext<ModelCatalogueState | null>(null);
const LOAD_ERROR = "Could not load models. Try again.";

/** Authenticated reads start on intent; advanced entries require explicit opt-in. */
export function ModelCatalogueProvider({ children }: { children: ReactNode }) {
  const state = useCatalogueState();
  return <ModelCatalogueContext.Provider value={state}>{children}</ModelCatalogueContext.Provider>;
}

function mergeModels(current: LibraryModel[], incoming: LibraryModel[]): LibraryModel[] {
  return Array.from(new Map([...current, ...incoming].map((model) => [JSON.stringify([model.provider, model.id]), model])).values());
}

function useCatalogueState(): ModelCatalogueState {
  const [data, setData] = useState(INITIAL_DATA);
  const pages = useRef(new Map<boolean, string | null>());
  const failed = useRef<PageQuery | null>(null);
  const load = useCatalogueLoader(setData, pages, failed);
  return {
    ...data,
    preload: () => load({ advanced: false, cursor: null }),
    preloadAdvanced: () => {
      load({ advanced: false, cursor: null });
      load({ advanced: true, cursor: null });
    },
    loadMore: (includeAdvanced = false) => {
      const ordinary = pages.current.get(false);
      const advanced = pages.current.get(true);
      if (ordinary) load({ advanced: false, cursor: ordinary });
      else if (includeAdvanced && advanced) load({ advanced: true, cursor: advanced });
    },
    retry: () => { if (failed.current) load(failed.current); },
  };
}

function useCatalogueLoader(setData: Dispatch<SetStateAction<CatalogueData>>,
  pages: RefObject<Map<boolean, string | null>>, failed: RefObject<PageQuery | null>) {
  const inFlight = useRef<PageQuery | null>(null);
  const pendingAdvanced = useRef(false);
  const router = useRouter();
  return useCallback(function load(query: PageQuery) {
    if (query.cursor === null && pages.current.has(query.advanced)) return;
    if (inFlight.current !== null) {
      if (query.advanced && !inFlight.current.advanced) pendingAdvanced.current = true;
      return;
    }
    inFlight.current = query;
    setData((current) => ({ ...current, status: CATALOGUE_STATUS.loading, error: null }));
    getModelLibraryAction(query.advanced, query.cursor).then((response) => {
      if (response.ok) {
        pages.current.set(query.advanced, response.data.next_cursor);
        failed.current = null;
        setData((current) => ({ ...current, models: mergeModels(current.models, response.data.models),
          status: CATALOGUE_STATUS.ready, error: null,
          nextCursor: query.advanced ? current.nextCursor : response.data.next_cursor,
          advancedNextCursor: query.advanced ? response.data.next_cursor : current.advancedNextCursor }));
      } else {
        failed.current = query;
        if (response.status === 401) router.push("/sign-in");
        else setData((current) => ({ ...current, status: CATALOGUE_STATUS.error, error: response.error }));
      }
    }).catch(() => {
      failed.current = query;
      setData((current) => ({ ...current, status: CATALOGUE_STATUS.error, error: LOAD_ERROR }));
    }).finally(() => {
      inFlight.current = null;
      if (pendingAdvanced.current && failed.current === null) {
        pendingAdvanced.current = false;
        load({ advanced: true, cursor: null });
      }
    });
  }, [failed, pages, router, setData]);
}

export function useModelCatalogue(): ModelCatalogueState {
  return useContext(ModelCatalogueContext) ?? FALLBACK_STATE;
}
