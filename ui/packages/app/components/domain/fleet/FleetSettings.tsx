"use client";

import type { FleetChange } from "@/lib/api/fleet-client";
import type { NativeFleetDetail } from "@/lib/api/fleet-detail";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { changeFleet, readFleetDetail } from "@/lib/api/fleet-client";
import { definiteRefusal, RequestFailed, requestErrorMessage } from "@/lib/api/live-request";
import { refreshRecentFleets } from "@/lib/api/use-recent-page";

/** The fields of a fleet a person changes from its page, each with one `PATCH F`. */
export type FleetSettingsValue = Pick<NativeFleetDetail, "name" | "etag" | "model_ref" | "http_egress" | "package_domains">;
type SettingKind = "name" | "model" | "network";
type ChangeResult = {ok: true} | {ok: false; message: string} | undefined;
export type FleetSettings = {
  value: FleetSettingsValue;
  pending: SettingKind | null;
  error: {kind: SettingKind; message: string} | null;
  /** Resolves once the daemon answered; the value moves only on its confirmation. */
  change: (kind: SettingKind, change: FleetChange) => Promise<ChangeResult>;
};

const NAME_CHANGED = "The fleet name changed. Review it and try again.";
const FleetSettingsContext = createContext<FleetSettings | null>(null);

export const FleetSettingsProvider = FleetSettingsContext;
export const useFleetSettings = () => useContext(FleetSettingsContext);

/** Use the last confirmed version to serialize changes from a fleet's page. */
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
  const publish = useCallback((row: FleetSettingsValue) => {
    const nameChanged = row.name !== confirmed.current.name;
    confirmed.current = {name: row.name, etag: row.etag, model_ref: row.model_ref, http_egress: row.http_egress, package_domains: row.package_domains};
    setValue(confirmed.current);
    if (nameChanged) refreshRecentFleets(workspace);
  }, [workspace]);
  const change = useCallback(async (kind: SettingKind, body: FleetChange): Promise<ChangeResult> => {
    if (busy.current) return;
    busy.current = true;
    setPending(kind); setError(null);
    const {signal} = owner.current;
    let row: FleetSettingsValue | undefined;
    let message = NAME_CHANGED;
    try {
      row = await changeFleet(workspace, fleet, confirmed.current.etag, body, signal);
    } catch (cause) {
      message = requestErrorMessage(cause);
      if ("name" in body && shouldReadCurrentFleet(cause)) {
        row = await readFleetDetail(workspace, fleet, signal).catch(() => undefined);
      }
    }
    busy.current = false;
    if (signal.aborted) return;
    setPending(null);
    if (row) {
      publish(row);
      if (!("name" in body) || row.name === body.name) return {ok: true};
    }
    setError({kind, message});
    return {ok: false, message};
  }, [workspace, fleet, publish]);
  return useMemo(() => ({value, pending, error, change}), [value, pending, error, change]);
}

function shouldReadCurrentFleet(cause: unknown): boolean {
  return (cause instanceof RequestFailed && cause.status === 412) || !definiteRefusal(cause);
}
