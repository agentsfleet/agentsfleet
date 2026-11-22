"use client";

import type { ReactNode } from "react";
import type { DurableTaskPage, DurableTaskSummary } from "@/lib/api/tasks";
import type { FleetSources } from "@/lib/fleet-sources";
import type { FleetTurn } from "@/lib/streaming/fleet-stream-row";
import type { Schedule } from "@/lib/api/draft-types";
import type { DurableTaskResource } from "./use-task-read";
import type { DurableTaskDetails } from "./use-task-details";
import type { RecordedChangeRepository } from "../conversation/summary/recorded-changes";
import type { DurableTaskReference } from "./task-references";
import { createContext, useCallback, useContext, useEffect, useLayoutEffect, useMemo, useState } from "react";
import { readDurableTasks } from "@/lib/api/tasks";
import { confirmed, useDurableTaskRead } from "./use-task-read";
import { useDurableTaskDetails } from "./use-task-details";
import { currentDurableTaskRef, recentDurableTaskIds } from "./task-references";

export type FleetDurableTaskProps = {
  workspaceId: string; fleetId: string; subject: string | null; revision: number;
  changeRepositories?: readonly RecordedChangeRepository[];
  files?: readonly {id: string; name: string}[];
  turns?: readonly FleetTurn[]; sources?: FleetSources; schedules?: readonly Schedule[];
  approvals?: {count: number; href: string};
  approvalRevision?: number;
  readOnly?: boolean;
};
export type FleetDurableTaskReadState = {scope: string; resource: DurableTaskResource<DurableTaskPage>; details: DurableTaskDetails; references: readonly DurableTaskReference[]; retry: () => void; retryDetails: () => void};
type ViewState = FleetDurableTaskProps & {read: FleetDurableTaskReadState | null};
/** What a linked-work card needs; stream turns and schedules do not reach it. */
type LinkedWorkState = {subject: string | null; read: FleetDurableTaskReadState | null};
const EMPTY_DETAILS: DurableTaskDetails = new Map();
const EMPTY_DURABLE_TASKS: readonly DurableTaskSummary[] = [];
const EMPTY_TURNS: readonly FleetTurn[] = [];
const DetailContext = createContext<DurableTaskDetails>(EMPTY_DETAILS);
const ViewContext = createContext<ViewState | null>(null);
const LinkedWorkContext = createContext<LinkedWorkState | null>(null);

/** Key only the data owner: revocation must not remount the conversation draft. */
export function FleetDurableTaskProvider({children, workspaceId, fleetId, subject, revision, turns, files, changeRepositories, sources, schedules, approvals, approvalRevision, readOnly = false}: FleetDurableTaskProps & {children: ReactNode}) {
  const props = {workspaceId, fleetId, subject, revision, turns, files, changeRepositories, sources, schedules, approvals, approvalRevision, readOnly};
  const [read, publish] = useState<FleetDurableTaskReadState | null>(null);
  const scope = `${props.subject}:${props.workspaceId}:${props.fleetId}`;
  const owned = props.subject && read?.scope === scope ? read : null;
  const details = owned && confirmed(owned.resource) ? owned.details : EMPTY_DETAILS;
  useEffect(() => {if (!subject) publish(null);}, [subject]);
  const view = useMemo<ViewState>(() => ({workspaceId, fleetId, subject, revision, turns, files, changeRepositories, sources, schedules, approvals, approvalRevision, readOnly, read: owned}),
    [workspaceId, fleetId, subject, revision, turns, files, changeRepositories, sources, schedules, approvals, approvalRevision, readOnly, owned]);
  const linked = useMemo<LinkedWorkState>(() => ({subject, read: owned}), [subject, owned]);
  return <DetailContext value={details}><LinkedWorkContext value={linked}><ViewContext value={view}>
    {props.subject ? <DurableTaskReadOwner key={scope} {...props} scope={scope} publish={publish} /> : null}
    {children}
  </ViewContext></LinkedWorkContext></DetailContext>;
}

export const useFleetDurableTaskDetails = () => useContext(DetailContext);
export const useFleetDurableTaskView = () => useContext(ViewContext);
export const useFleetLinkedWork = () => useContext(LinkedWorkContext);

function DurableTaskReadOwner({workspaceId, fleetId, revision, turns = EMPTY_TURNS, scope, publish}: FleetDurableTaskProps & {scope: string; publish: (read: FleetDurableTaskReadState) => void}) {
  const load = useCallback((signal: AbortSignal) => readDurableTasks(workspaceId, fleetId, signal), [workspaceId, fleetId]);
  const {resource, retry} = useDurableTaskRead(load, revision);
  const page = confirmed(resource);
  const tasks = page?.items ?? EMPTY_DURABLE_TASKS;
  const historyKey = page ? recentDurableTaskIds(turns, tasks).join(",") : "";
  const references = useMemo<DurableTaskReference[]>(() => [...tasks.map(currentDurableTaskRef),
    ...historyKey.split(",").filter(Boolean).map(id => ({kind: "historical" as const, id}))], [tasks, historyKey]);
  const {details, retry: retryDetails} = useDurableTaskDetails(workspaceId, fleetId, references);
  useLayoutEffect(() => {publish({scope, resource, details, references, retry, retryDetails});}, [scope, resource, details, references, retry, retryDetails, publish]);
  return null;
}
