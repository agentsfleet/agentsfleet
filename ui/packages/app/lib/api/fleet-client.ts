// What the browser asks of one fleet, through the same-origin `/live` proxy:
// create it from a first message, change one field, and read and trim the
// lists beside its chat. Every call takes its caller's signal and names the
// exact body the daemon's Interfaces declare.

import { z } from "zod";
import { Instant } from "./decode";
import { fleetPath, fleetsPath } from "./fleet-paths";
import { FleetDetailSchema, GrantSchema, TriggerSchema, type FleetGrant, type FleetTrigger, type HttpEgress, type NativeFleetDetail } from "./fleet-detail";
import { liveRequest, REQUEST_METHOD } from "./live-request";
import { walkList } from "./list-walk";
import { ScheduleSchema, type Schedule } from "./schedule-types";

/** The memory category every run's prompt carries: what a fleet pinned. */
export const PINNED_CATEGORY = "core";
// The memory list's accepted page ceiling (`GET F/memories`).
const MEMORY_PAGE_LIMIT = 100;

export type CreateFleet = {message: string; operation_id: string; model_ref?: string};
export type CreatedFleet = {fleetId: string; turnId: string; replayed: boolean};
export type FleetChange = {model_ref: string} | {http_egress: HttpEgress} | {package_domains: boolean};
export type PinnedMemory = z.infer<typeof MemoryEntrySchema>;

// The answer names a whole fleet; the page reads that fleet itself, so only
// its id is held here and a field the page does not need cannot fail a create.
const CreatedSchema = z.object({fleet: z.object({id: z.uuidv7()}), accepted: z.object({turn_id: z.uuidv7()}), replayed: z.boolean()});
const SchedulesSchema = z.object({schedules: z.array(ScheduleSchema)});
const TriggersSchema = z.object({items: z.array(TriggerSchema)});
const GrantsSchema = z.object({items: z.array(GrantSchema)});
const MemoryEntrySchema = z.object({key: z.string(), content: z.string(), category: z.string(), updated_at: Instant,
  visibility: z.enum(["fleet", "workspace"]), writer_fleet_id: z.string()});
const MemoryPageSchema = z.object({items: z.array(MemoryEntrySchema), total: z.number().int().nonnegative().nullable(), next_cursor: z.string().nullable()});

/** `POST W/fleets`: the fleet and its first turn, in one call; a repeat of one operation answers the same fleet. */
export async function createFleet(workspace: string, body: CreateFleet, signal: AbortSignal): Promise<CreatedFleet> {
  const created = await liveRequest({path: `/live${fleetsPath(workspace)}`, method: REQUEST_METHOD.post, body}, CreatedSchema, signal);
  return {fleetId: created.fleet.id, turnId: created.accepted.turn_id, replayed: created.replayed};
}

/** `PATCH F` with the version the caller read; the answer is the fleet as it now stands. */
export function changeFleet(workspace: string, fleet: string, etag: string, change: FleetChange, signal: AbortSignal): Promise<NativeFleetDetail> {
  return liveRequest({path: livePath(workspace, fleet), method: REQUEST_METHOD.patch, headers: {"If-Match": etag}, body: change},
    FleetDetailSchema, signal);
}

export async function readSchedules(workspace: string, fleet: string, signal: AbortSignal): Promise<Schedule[]> {
  return (await liveRequest({path: livePath(workspace, fleet, "schedules")}, SchedulesSchema, signal)).schedules;
}

export async function readTriggers(workspace: string, fleet: string, signal: AbortSignal): Promise<FleetTrigger[]> {
  return (await liveRequest({path: livePath(workspace, fleet, "triggers")}, TriggersSchema, signal)).items;
}

export async function readGrants(workspace: string, fleet: string, signal: AbortSignal): Promise<FleetGrant[]> {
  return (await liveRequest({path: livePath(workspace, fleet, "integration-grants")}, GrantsSchema, signal)).items;
}

/** The fleet's own `core` entries: other categories and other fleets' shared entries are never pinned for it. */
export async function readPinnedMemory(workspace: string, fleet: string, signal: AbortSignal): Promise<PinnedMemory[]> {
  const {items} = await walkList("memory list", cursor => {
    const query = new URLSearchParams({limit: String(MEMORY_PAGE_LIMIT)});
    if (cursor !== null) query.set("starting_after", cursor);
    return liveRequest({path: `${livePath(workspace, fleet, "memories")}?${query}`}, MemoryPageSchema, signal);
  });
  return items.filter(entry => entry.category === PINNED_CATEGORY && entry.writer_fleet_id === fleet);
}

export async function removeTrigger(workspace: string, fleet: string, id: string, signal: AbortSignal): Promise<void> {
  await liveRequest({path: livePath(workspace, fleet, "triggers", id), method: REQUEST_METHOD.delete}, z.null(), signal);
}

export async function revokeGrant(workspace: string, fleet: string, id: string, signal: AbortSignal): Promise<void> {
  await liveRequest({path: livePath(workspace, fleet, "integration-grants", id), method: REQUEST_METHOD.delete}, z.null(), signal);
}

export async function forgetPinnedMemory(workspace: string, fleet: string, key: string, signal: AbortSignal): Promise<void> {
  await liveRequest({path: livePath(workspace, fleet, "memories", key), method: REQUEST_METHOD.delete}, z.null(), signal);
}

function livePath(workspace: string, fleet: string, ...rest: string[]): string {
  return [`/live${fleetPath(workspace, fleet)}`, ...rest.map(encodeURIComponent)].join("/");
}
