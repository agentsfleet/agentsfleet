"use client";

import { createContext, useContext } from "react";
import type { FleetModelOwner, useFleetModel } from "../models/useFleetModel";

export type SetupLocation = {workspace: string; fleet: string; revision: string};
type FleetSetupState = {location: SetupLocation; modelOwner: FleetModelOwner | null; modelChanging: boolean; modelState: ReturnType<typeof useFleetModel>};
export const FleetSetupContext = createContext<FleetSetupState | null>(null);
export const useFleetSetup = () => useContext(FleetSetupContext);
