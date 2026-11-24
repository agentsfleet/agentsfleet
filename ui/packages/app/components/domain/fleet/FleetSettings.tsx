"use client";

import type { FleetChange } from "@/lib/api/fleet-client";
import type { NativeFleetDetail } from "@/lib/api/fleet-detail";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { changeFleet } from "@/lib/api/fleet-client";
import { requestErrorMessage } from "@/lib/api/live-request";

/** The fields of a fleet a person changes from its page, each with one `PATCH F`. */
export type FleetSettingsValue = Pick<NativeFleetDetail, "etag" | "model_ref" | "http_egress" | "package_domains">;
export type SettingKind = "model" | "network";
export type FleetSettings = {
  value: FleetSettingsValue;
  pending: SettingKind | null;
  error: {kind: SettingKind; message: string} | null;
  /** Resolves once the daemon answered; the value moves only on its confirmation. */
  change: (kind: SettingKind, change: FleetChange) => Promise<void>;
};

const FleetSettingsContext = createContext<FleetSettings | null>(null);

export const FleetSettingsProvider = FleetSettingsContext;
export const useFleetSettings = () => useContext(FleetSettingsContext);

/** One change at a time, sent with the version last confirmed. A refusal leaves every field as it was. */
export function useFleetSettingsOwner(workspace: string, fleet: string, initial: FleetSettingsValue): FleetSettings {
  const [value, setValue] = useState(initial);
  const [pending, setPending] = useState<SettingKind | null>(null);
  const [error, setError] = useState<FleetSettings["error"]>(null);
  const confirmed = useRef(initial);
  const busy = useRef(false);
  // Unmounting cancels a change on the wire; its late answer then moves nothing.
  const owner = useRef(new AbortController());
  useEffect(() => {
    const controller = new AbortController();
    owner.current = controller;
    return () => controller.abort();
  }, []);
  const change = useCallback(async (kind: SettingKind, body: FleetChange): Promise<void> => {
    if (busy.current) return;
    busy.current = true;
    setPending(kind); setError(null);
    try {
      const row = await changeFleet(workspace, fleet, confirmed.current.etag, body, owner.current.signal);
      confirmed.current = {etag: row.etag, model_ref: row.model_ref, http_egress: row.http_egress, package_domains: row.package_domains};
      setValue(confirmed.current);
    } catch (cause) {
      setError({kind, message: requestErrorMessage(cause)});
    } finally {
      busy.current = false;
      setPending(null);
    }
  }, [workspace, fleet]);
  return useMemo(() => ({value, pending, error, change}), [value, pending, error, change]);
}
