"use client";

import type { Dispatch, ReactNode, SetStateAction } from "react";
import type { FleetGrant, FleetTrigger } from "@/lib/api/fleet-detail";
import type { PinnedMemory } from "@/lib/api/fleet-client";
import type { Schedule } from "@/lib/api/schedule-types";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import { forgetPinnedMemory, readGrants, readPinnedMemory, readSchedules, readTriggers, removeTrigger, revokeGrant } from "@/lib/api/fleet-client";
import { requestErrorMessage } from "@/lib/api/live-request";
import { deleteSchedule, syncSchedule } from "@/lib/api/schedule-controls";

/** One list beside the chat: read, or not readable right now. */
export type ListResource<T> = {kind: "loading"} | {kind: "ready"; items: readonly T[]} | {kind: "failed"; message: string};
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

/** Reads the fleet's schedules, triggers, grants and pinned memory once per reload; the page never asks for a draft or a setup. */
export function FleetListsProvider({workspace, fleet, children}: {workspace: string; fleet: string; children: ReactNode}) {
  const [schedules, setSchedules] = useState<ListResource<Schedule>>(LOADING);
  const [triggers, setTriggers] = useState<ListResource<FleetTrigger>>(LOADING);
  const [grants, setGrants] = useState<ListResource<FleetGrant>>(LOADING);
  const [pinned, setPinned] = useState<ListResource<PinnedMemory>>(LOADING);
  const [revision, setRevision] = useState(0);
  const owner = useRef(new AbortController());
  useEffect(() => {
    const controller = new AbortController();
    owner.current = controller;
    const {signal} = controller;
    void settle(readSchedules(workspace, fleet, signal), setSchedules, signal);
    void settle(readTriggers(workspace, fleet, signal), setTriggers, signal);
    void settle(readGrants(workspace, fleet, signal), setGrants, signal);
    void settle(readPinnedMemory(workspace, fleet, signal), setPinned, signal);
    return () => controller.abort();
  }, [workspace, fleet, revision]);
  const reload = useCallback(() => setRevision(value => value + 1), []);
  const remove = useCallback(async (removal: Removal): Promise<string | null> => {
    try {
      const left = await removeRemotely(workspace, fleet, removal, owner.current.signal);
      dropRow(removal, left, {setSchedules, setTriggers, setGrants, setPinned});
      return null;
    } catch (cause) {
      return requestErrorMessage(cause);
    }
  }, [workspace, fleet]);
  const retrySchedule = useCallback(async (schedule: Schedule): Promise<string | null> => {
    try {
      const row = await syncSchedule(workspace, schedule, owner.current.signal);
      setSchedules(current => replace(current, item => item.schedule_id === row.schedule_id, row));
      return null;
    } catch (cause) {
      return requestErrorMessage(cause);
    }
  }, [workspace]);
  const value = useMemo(() => ({schedules, triggers, grants, pinned, reload, remove, retrySchedule}),
    [schedules, triggers, grants, pinned, reload, remove, retrySchedule]);
  return <Context value={value}>{children}</Context>;
}

type Setters = {
  setSchedules: Dispatch<SetStateAction<ListResource<Schedule>>>; setTriggers: Dispatch<SetStateAction<ListResource<FleetTrigger>>>;
  setGrants: Dispatch<SetStateAction<ListResource<FleetGrant>>>; setPinned: Dispatch<SetStateAction<ListResource<PinnedMemory>>>;
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

async function settle<T>(read: Promise<readonly T[]>, publish: (resource: ListResource<T>) => void, signal: AbortSignal): Promise<void> {
  try {
    const items = await read;
    if (!signal.aborted) publish({kind: "ready", items});
  } catch (cause) {
    if (!signal.aborted) publish({kind: "failed", message: requestErrorMessage(cause)});
  }
}

function replace<T>(current: ListResource<T>, matches: (row: T) => boolean, next: T | null): ListResource<T> {
  if (current.kind !== "ready") return current;
  return {kind: "ready", items: current.items.flatMap(row => !matches(row) ? [row] : next === null ? [] : [next])};
}
