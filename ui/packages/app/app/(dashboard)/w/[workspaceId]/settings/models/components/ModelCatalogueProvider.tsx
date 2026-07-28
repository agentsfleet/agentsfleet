"use client";

import { createContext, useCallback, useContext, useRef, useState, type ReactNode } from "react";
import { useRouter } from "next/navigation";
import type { LibraryModel } from "@/lib/api/model_library";
import { getModelLibraryAction } from "../actions";
import { CATALOGUE_STATUS, type CatalogueStatus } from "./catalogue-status";

// The global model library, loaded on INTENT rather than on mount.
//
// It used to ride a single client-side fetch in a mount effect, so every visit
// to the Models page paid for a catalogue most visits never consulted: the
// registry rows carry their own server-provided rates, and the catalogue is
// only a fallback for a row the server did not price. The pickers that truly
// need it live inside the Add and Edit dialogs, so it now loads when a user
// shows intent to open one.
//
// The read still goes through the token-minting Server Action — GET /v1/models
// is bearer-authed and the token never reaches the browser. A 401 means the
// session, not the catalogue, so the user routes to sign-in. Any other failure
// degrades pickers to free-text model entry.

// CATALOGUE_STATUS lives in ./catalogue-status — ProviderModelSelect keys its
// control shape off it, and this module gets stubbed wholesale in tests.

export type ModelCatalogueState = {
  models: LibraryModel[];
  status: CatalogueStatus;
  /** Request the catalogue. Idempotent, single-flight, safe to call on hover. */
  preload: () => void;
};

// Consumers rendered outside a provider degrade to free-text entry rather than
// throwing — the catalogue is an enhancement, never a hard dependency.
const FALLBACK_STATE: ModelCatalogueState = {
  models: [],
  status: CATALOGUE_STATUS.error,
  preload: () => {},
};

const ModelCatalogueContext = createContext<ModelCatalogueState | null>(null);

/**
 * Whether a hover may speculate. A coarse pointer has no true hover — a touch
 * that lands on a control is already a press, so "hover" prefetch there is just
 * an unconditional fetch wearing a different name. Save-Data is the user asking
 * not to spend bytes on a maybe.
 *
 * Focus and open are NOT gated by this: both are deliberate, so the request is
 * wanted rather than speculative.
 */
export function maySpeculateOnHover(): boolean {
  if (typeof window === "undefined") return false;
  // Typed non-nullish, but absent in some test environments — a `typeof` probe
  // rather than an optional chain, which the type system reads as dead.
  if (typeof window.matchMedia === "function" && window.matchMedia("(pointer: coarse)").matches) {
    return false;
  }
  const connection = (navigator as { connection?: { saveData?: boolean } }).connection;
  return connection?.saveData !== true;
}

export function ModelCatalogueProvider({ children }: { children: ReactNode }) {
  const [models, setModels] = useState<LibraryModel[]>([]);
  const [status, setStatus] = useState<CatalogueStatus>(CATALOGUE_STATUS.idle);
  const router = useRouter();

  // Single-flight, held in a ref because a hover storm must not re-render on
  // every attempt.
  //
  // This is also what delivers the ordering property, which is why there is no
  // request id beside it: a monotonic generation resolves a race between two
  // in-flight reads, and this guard means there is never a second one to race.
  // `inFlight` is only cleared in `.finally`, which runs after `.then`/`.catch`
  // — so every handler below is provably the newest request's. `useStoredSecrets`
  // is the sibling case that does need a generation: open/close/reopen genuinely
  // overlap there, because it takes no single-flight guard.
  const inFlight = useRef(false);

  const preload = useCallback(() => {
    // Hover, focus, and open all call this, often within the same gesture.
    // Without the guard one deliberate click could issue three identical
    // catalogue reads.
    if (inFlight.current) return;
    if (status === CATALOGUE_STATUS.ready) return;

    inFlight.current = true;
    setStatus(CATALOGUE_STATUS.loading);

    getModelLibraryAction()
      .then((res) => {
        if (res.ok) {
          setModels(res.data.models);
          setStatus(CATALOGUE_STATUS.ready);
          return;
        }
        // An expired session must not become a silent free-text degrade — the
        // user would hand-type model ids into a signed-out page.
        if (res.status === 401) {
          router.push("/sign-in");
          return;
        }
        setStatus(CATALOGUE_STATUS.error);
      })
      .catch(() => {
        setStatus(CATALOGUE_STATUS.error);
      })
      .finally(() => {
        inFlight.current = false;
      });
  }, [router, status]);

  return (
    <ModelCatalogueContext.Provider value={{ models, status, preload }}>
      {children}
    </ModelCatalogueContext.Provider>
  );
}

/** Read the catalogue. Returns a safe degraded state if no provider is mounted. */
export function useModelCatalogue(): ModelCatalogueState {
  return useContext(ModelCatalogueContext) ?? FALLBACK_STATE;
}
