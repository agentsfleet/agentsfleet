"use client";

import type { WorkspaceModel } from "@/lib/api/workspace-models";
import { createContext, useContext } from "react";

/** What the composer's model picker shows and does. On a fleet a choice is one
 * `PATCH F`; on New fleet it is the `model_ref` the first message carries. */
export type FleetModelChoice = {
  models: readonly WorkspaceModel[];
  selected: string | null;
  label: string;
  /** A choice is on its way to the daemon: the composer waits for it. */
  changing: boolean;
  error: string | null;
  select: (ref: string) => void;
};

export const FleetModelContext = createContext<FleetModelChoice | null>(null);
export const useFleetModelChoice = () => useContext(FleetModelContext);
