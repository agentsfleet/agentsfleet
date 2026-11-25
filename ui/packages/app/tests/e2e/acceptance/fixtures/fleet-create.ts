/**
 * fleet-create.ts — a fleet is made by its first message.
 *
 * `POST W/fleets {message, operation_id, model_ref?, name?}` writes the fleet,
 * its thread and the first turn in one transaction, so every fixture fleet is
 * born `active` with one queued turn a runner answers. A fleet holds no
 * instructions: what a probe must do rides in each message it is sent
 * (`probe.ts`).
 */
import type { NativeFleetDetail } from "@/lib/api/fleet-detail";
import type { ThreadPage } from "@/lib/api/turns";
import { v7 as uuidv7 } from "uuid";
import { z } from "zod";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { ModelsSchema } from "@/lib/api/workspace-models";
import { clientFor, FixtureHttpError, type ClientHandle } from "./api-client";
import { turnHasSettled } from "./execution";

// The first message's run: the runner queue plus one provider round trip.
const FIRST_TURN_TIMEOUT_MS = 180_000;
const POLL_INTERVAL_MS = 2_000;
const MODEL_PAGE_LIMIT = 100;
const MODEL_PAGE_MAX = 100;
const PLATFORM_DEFAULT_REF = "platform-default";
/** `UZ-AGT-006`: a fleet already holds the name (`af_core::error_code`). */
const NAME_TAKEN = "UZ-AGT-006";
/** Parallel workers share a workspace and the daemon refuses a taken name, so a
 * purpose name is tried with a counter after it; the bound only ends a loop
 * that a daemon refusing every name would never end. */
const NAME_ATTEMPTS = 50;
const CreatedSchema = z.object({fleet: FleetDetailSchema, accepted: z.object({turn_id: z.string()}), replayed: z.boolean()});

export type CreatedFixtureFleet = {readonly fleet: NativeFleetDetail; readonly turnId: string};
export type CreateFleetOpts = {readonly message: string; readonly name?: string; readonly modelRef?: string};

/** Creates a fleet through the API. A purpose name a sibling already holds is tried again as `name-2`, `name-3`, … */
export async function createFixtureFleet(handle: ClientHandle, workspace: string, opts: CreateFleetOpts): Promise<CreatedFixtureFleet> {
  const client = clientFor(handle);
  for (let attempt = 1; attempt <= NAME_ATTEMPTS; attempt += 1) {
    const name = opts.name === undefined ? undefined : attempt === 1 ? opts.name : `${opts.name}-${attempt}`;
    try {
      const created = CreatedSchema.parse(await client.post<unknown>(`/v1/workspaces/${workspace}/fleets`, {
        message: opts.message, operation_id: uuidv7(),
        ...(opts.modelRef === undefined ? {} : {model_ref: opts.modelRef}), ...(name === undefined ? {} : {name}),
      }));
      return {fleet: created.fleet, turnId: created.accepted.turn_id};
    } catch (error) {
      if (!(error instanceof FixtureHttpError && error.status === 409 && error.code === NAME_TAKEN) || name === undefined) throw error;
    }
  }
  throw new Error(`every name from ${opts.name ?? "the default"} to -${NAME_ATTEMPTS} is taken in workspace ${workspace}`);
}

/**
 * Waits out the fleet's first turn, so a spec that writes the page's stream
 * itself never meets a run still going behind it: a turn the page renders as
 * running would never settle on screen there. A turn that ended asking a
 * person has settled too: no run is left behind it.
 */
export async function awaitFirstTurnSettled(handle: ClientHandle, workspace: string, fleetId: string, turnId: string): Promise<void> {
  const client = clientFor(handle);
  const deadline = Date.now() + FIRST_TURN_TIMEOUT_MS;
  for (;;) {
    const page = await client.get<ThreadPage>(`/v1/workspaces/${workspace}/fleets/${encodeURIComponent(fleetId)}/messages`);
    const status = page.items.find((item) => item.turn_id === turnId)?.status;
    if (status !== undefined && turnHasSettled(status)) return;
    if (Date.now() > deadline) throw new Error(`fleet ${fleetId}'s first turn never settled; last status ${status ?? "absent"}`);
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
}

export async function readFixtureFleet(handle: ClientHandle, workspace: string, fleetId: string): Promise<NativeFleetDetail> {
  return FleetDetailSchema.parse(await clientFor(handle).get<unknown>(`/v1/workspaces/${workspace}/fleets/${encodeURIComponent(fleetId)}`));
}

/** Prefer the live platform model over entries other tests create with dummy keys. */
export async function runtimeModelRef(handle: ClientHandle, workspace: string): Promise<string> {
  const client = clientFor(handle);
  let cursor: string | null = null;
  let fallback: string | undefined;
  for (let page = 0; page < MODEL_PAGE_MAX; page++) {
    const query = new URLSearchParams({ limit: String(MODEL_PAGE_LIMIT) });
    if (cursor) query.set("starting_after", cursor);
    const models = ModelsSchema.parse(await client.get<unknown>(`/v1/workspaces/${workspace}/models?${query}`));
    const eligible = models.items.filter((entry) => entry.eligibility.runtime);
    const model = eligible.find((entry) => entry.ref === PLATFORM_DEFAULT_REF);
    if (model) return model.ref;
    fallback ??= eligible[0]?.ref;
    if (!models.next_cursor || models.next_cursor === cursor) break;
    cursor = models.next_cursor;
  }
  if (fallback) return fallback;
  throw new Error("No runtime model in the workspace for an acceptance fixture");
}
