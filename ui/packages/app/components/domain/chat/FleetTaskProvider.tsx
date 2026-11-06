"use client";

import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useState, type ReactNode } from "react";
import { readTasks, type TaskPage, type TaskSummary } from "@/lib/api/tasks";
import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetEvent } from "@/lib/streaming/fleet-stream-row";
import { confirmed, useTaskRead, type TaskResource } from "./use-task-read";
import { useTaskDetails, type TaskDetails } from "./use-task-details";
import { currentTaskRef, recentTaskIds, type TaskReference } from "./task-references";

export type FleetTaskProps = {workspaceId: string; fleetId: string; subject: string | null; revision: number; events?: readonly FleetEvent[]; sources?: FleetSources};
type ReadState = {scope: string; resource: TaskResource<TaskPage>; details: TaskDetails; retry: () => void; retryDetails: () => void};
type ViewState = FleetTaskProps & {read: ReadState | null};
const EMPTY_DETAILS: TaskDetails = new Map();
const EMPTY_TASKS: readonly TaskSummary[] = [];
const EMPTY_EVENTS: readonly FleetEvent[] = [];
const DetailContext = createContext<TaskDetails>(EMPTY_DETAILS);
const ViewContext = createContext<ViewState | null>(null);
export const useFleetTaskDetails = () => useContext(DetailContext);
export const useFleetTaskView = () => useContext(ViewContext);

/** Key only the data owner: revocation must not remount the conversation draft. */
export function FleetTaskProvider({children, workspaceId, fleetId, subject, revision, events, sources}: FleetTaskProps & {children: ReactNode}) {
  const props = {workspaceId, fleetId, subject, revision, events, sources};
  const [read, publish] = useState<ReadState | null>(null);
  const scope = `${props.subject}:${props.workspaceId}:${props.fleetId}`;
  const owned = props.subject && read?.scope === scope ? read : null;
  const details = owned && confirmed(owned.resource) ? owned.details : EMPTY_DETAILS;
  useEffect(() => {if (!subject) publish(null);}, [subject]);
  const view = useMemo<ViewState>(() => ({workspaceId, fleetId, subject, revision, events, sources, read: owned}),
    [workspaceId, fleetId, subject, revision, events, sources, owned]);
  return <DetailContext value={details}><ViewContext value={view}>
    {props.subject ? <TaskReadOwner key={scope} {...props} scope={scope} publish={publish} /> : null}
    {children}
  </ViewContext></DetailContext>;
}

function TaskReadOwner({workspaceId, fleetId, revision, events = EMPTY_EVENTS, scope, publish}: FleetTaskProps & {scope: string; publish: (read: ReadState) => void}) {
  const load = useCallback((signal: AbortSignal) => readTasks(workspaceId, fleetId, signal), [workspaceId, fleetId]);
  const {resource, retry} = useTaskRead(load, revision);
  const page = confirmed(resource);
  const tasks = page?.items ?? EMPTY_TASKS;
  const historyKey = page ? recentTaskIds(events, tasks).join(",") : "";
  const references = useMemo<TaskReference[]>(() => [...tasks.map(currentTaskRef),
    ...historyKey.split(",").filter(Boolean).map(id => ({kind: "historical" as const, id}))], [tasks, historyKey]);
  const {details, retry: retryDetails} = useTaskDetails(workspaceId, fleetId, references);
  useLayoutEffect(() => {publish({scope, resource, details, retry, retryDetails});}, [scope, resource, details, retry, retryDetails, publish]);
  return null;
}
