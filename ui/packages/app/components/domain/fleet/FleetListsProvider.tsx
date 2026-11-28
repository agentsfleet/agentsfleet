"use client";

import type { ReactNode } from "react";
import type { FleetGrant, FleetTrigger, NativeFleetDetail } from "@/lib/api/fleet-detail";
import type { PinnedMemory } from "@/lib/api/fleet-client";
import type { Schedule } from "@/lib/api/schedule-types";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { forgetPinnedMemory, readGrants, readPinnedMemory, readSchedules, readTriggers, removeTrigger, revokeGrant } from "@/lib/api/fleet-client";
import { requestErrorMessage } from "@/lib/api/live-request";
import { deleteSchedule, syncSchedule } from "@/lib/api/schedule-controls";

/** One list beside the chat: read, or not readable right now. */
export type ListResource<T> = {kind: "loading"} | {kind: "ready"; items: readonly T[]} | {kind: "failed"; message: string};
export type FleetListSeed = Partial<Pick<NativeFleetDetail, "schedules" | "triggers" | "grants">>;
export type Removal = {kind: "schedule"; schedule: Schedule} | {kind: "trigger"; trigger: FleetTrigger}
  | {kind: "grant"; grant: FleetGrant} | {kind: "memory"; memory: PinnedMemory};
export type FleetLists = {
  schedules: ListResource<Schedule>;
  triggers: ListResource<FleetTrigger>;
  grants: ListResource<FleetGrant>;
  pinned: ListResource<PinnedMemory>;
  /** Reads all four lists again, cancelling a read still out. */
  reload: () => void;
  /** Resolves with the refusal's sentence, or null once the row is gone. */
  remove: (removal: Removal) => Promise<string | null>;
  /** Asks the scheduler to register a schedule again; resolves like `remove`. */
  retrySchedule: (schedule: Schedule) => Promise<string | null>;
};

const LOADING = {kind: "loading"} as const;
const Context = createContext<FleetLists | null>(null);

export const useFleetLists = () => useContext(Context);

/** Reads the lists again each time another turn settles: a run may have pinned, scheduled or added a trigger. */
export function useReloadListsAfter(settledTurns: number): void {
  const reload = useFleetLists()?.reload;
  const seen = useRef(settledTurns);
  useEffect(() => {
    const grew = settledTurns > seen.current;
    seen.current = settledTurns;
    if (grew) reload?.();
  }, [settledTurns, reload]);
}

/** Opens with the lists on the fleet detail. Pinned memory lives on its own route; a reload reads all four. */
export function FleetListsProvider({workspace, fleet, initial, children}: {workspace: string; fleet: string; initial?: FleetListSeed; children: ReactNode}) {
  const [schedules, loadSchedules, setSchedules] = useFleetList(initial?.schedules);
  const [triggers, loadTriggers, setTriggers] = useFleetList(initial?.triggers);
  const [grants, loadGrants, setGrants] = useFleetList(initial?.grants);
  const [pinned, loadPinned, setPinned] = useFleetList<PinnedMemory>();
  const [revision, setRevision] = useState(0);
  const readScheduleList = revision > 0 || initial?.schedules === undefined;
  const readTriggerList = revision > 0 || initial?.triggers === undefined;
  const readGrantList = revision > 0 || initial?.grants === undefined;
  const owner = useRef(new AbortController());
  useEffect(() => {
    const controller = new AbortController();
    owner.current = controller;
    return () => controller.abort();
  }, [workspace, fleet]);
  useEffect(() => {
    const controller = new AbortController();
    const {signal} = controller;
    if (readScheduleList) void loadSchedules(readSchedules(workspace, fleet, signal), signal);
    if (readTriggerList) void loadTriggers(readTriggers(workspace, fleet, signal), signal);
    if (readGrantList) void loadGrants(readGrants(workspace, fleet, signal), signal);
    void loadPinned(readPinnedMemory(workspace, fleet, signal), signal);
    return () => controller.abort();
  }, [workspace, fleet, revision, readScheduleList, readTriggerList, readGrantList, loadSchedules, loadTriggers, loadGrants, loadPinned]);
  const reload = useCallback(() => setRevision(value => value + 1), []);
  const remove = useCallback(async (removal: Removal): Promise<string | null> => {
    try {
      const {signal} = owner.current;
      const left = await removeRemotely(workspace, fleet, removal, signal);
      signal.throwIfAborted();
      dropRow(removal, left, {setSchedules, setTriggers, setGrants, setPinned});
      return null;
    } catch (cause) {
      return requestErrorMessage(cause);
    }
  }, [workspace, fleet, setSchedules, setTriggers, setGrants, setPinned]);
  const retrySchedule = useCallback(async (schedule: Schedule): Promise<string | null> => {
    try {
      const {signal} = owner.current;
      const row = await syncSchedule(workspace, schedule, signal);
      signal.throwIfAborted();
      setSchedules(current => replace(current, item => item.schedule_id === row.schedule_id, row));
      return null;
    } catch (cause) {
      return requestErrorMessage(cause);
    }
  }, [workspace, setSchedules]);
  const value = useMemo(() => ({schedules, triggers, grants, pinned, reload, remove, retrySchedule}),
    [schedules, triggers, grants, pinned, reload, remove, retrySchedule]);
  return <Context value={value}>{children}</Context>;
}

type Change<T> = (resource: ListResource<T>) => ListResource<T>;

type Setters = {
  setSchedules: (change: Change<Schedule>) => void; setTriggers: (change: Change<FleetTrigger>) => void;
  setGrants: (change: Change<FleetGrant>) => void; setPinned: (change: Change<PinnedMemory>) => void;
};

// A 200 from a schedule's delete is a removal the scheduler has yet to confirm, answered with the row; only a 204 means it is gone.
async function removeRemotely(workspace: string, fleet: string, removal: Removal, signal: AbortSignal): Promise<Schedule | null> {
  switch (removal.kind) {
    case "schedule": return deleteSchedule(workspace, removal.schedule, signal);
    case "trigger": await removeTrigger(workspace, fleet, removal.trigger.id, signal); return null;
    case "grant": await revokeGrant(workspace, fleet, removal.grant.id, signal); return null;
    case "memory": await forgetPinnedMemory(workspace, fleet, removal.memory.key, signal); return null;
  }
}

function dropRow(removal: Removal, left: Schedule | null, lists: Setters): void {
  switch (removal.kind) {
    case "schedule": lists.setSchedules(current => replace(current, row => row.schedule_id === removal.schedule.schedule_id, left)); break;
    case "trigger": lists.setTriggers(current => replace(current, row => row.id === removal.trigger.id, null)); break;
    case "grant": lists.setGrants(current => replace(current, row => row.id === removal.grant.id, null)); break;
    case "memory": lists.setPinned(current => replace(current, row => row.key === removal.memory.key, null)); break;
  }
}

/** A GET can finish after a confirmed change. Apply those changes to its answer as well as the visible rows. */
function useFleetList<T>(initial?: readonly T[]) {
  const [resource, publish] = useState<ListResource<T>>(() => initial ? {kind: "ready", items: initial} : LOADING);
  const pending = useRef<Change<T>[] | null>(null);
  const load = useCallback(async (read: Promise<readonly T[]>, signal: AbortSignal) => {
    const changes: Change<T>[] = [];
    pending.current = changes;
    try {
      const items = await read;
      if (!signal.aborted) publish(changes.reduce<ListResource<T>>((value, change) => change(value), {kind: "ready", items}));
    } catch (cause) {
      if (!signal.aborted) publish({kind: "failed", message: requestErrorMessage(cause)});
    } finally {
      if (pending.current === changes) pending.current = null;
    }
  }, []);
  const update = useCallback((change: Change<T>) => {
    pending.current?.push(change);
    publish(change);
  }, []);
  return [resource, load, update] as const;
}

function replace<T>(current: ListResource<T>, matches: (row: T) => boolean, next: T | null): ListResource<T> {
  if (current.kind !== "ready") return current;
  return {kind: "ready", items: current.items.flatMap(row => !matches(row) ? [row] : next === null ? [] : [next])};
}
