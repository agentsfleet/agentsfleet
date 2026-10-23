import type { ClientHandle } from "./api-client";
// Node APIs match the shared Playwright/Vitest types without augmenting browser fetch.
import {readFile, stat} from "node:fs/promises";
import { z } from "zod";
import { clientFor, FixtureHttpError } from "./api-client";
import { FleetSchema } from "@/lib/api/draft-types";
import { HTTP_STATUS_NOT_FOUND } from "@/lib/api/errors";
import { FIXTURE_KEY } from "./constants";
import { CONTINUITY_FLEET_PREFIX } from "./continuity-scenario";
import { loadWorktreeEnv } from "./env-loader";
import { assertDestructiveTargetIsSafe, purgeFixtureFleet } from "./teardown";

const MAX_SAVED_SETUPS = 4;
const MAX_MANIFEST_BYTES = 16 * 1024;
const Identifiers = z.strictObject({
  workspace: z.uuidv7(), name: z.string().startsWith(CONTINUITY_FLEET_PREFIX),
  fleet: z.uuidv7().nullable(), drafts: z.array(z.uuidv7()).max(MAX_SAVED_SETUPS),
});

/** Only the explicitly supplied development fixture identifiers may be removed. */
export async function cleanupContinuity(input: unknown, handle: ClientHandle = FIXTURE_KEY.regular): Promise<void> {
  assertDestructiveTargetIsSafe();
  const identifiers = Identifiers.parse(input);
  const client = clientFor(handle);
  const scope = `/v1/workspaces/${identifiers.workspace}`;
  const fleetId = identifiers.fleet;
  if (fleetId) await allowMissing(async () => {
    const fleet = FleetSchema.parse(await client.get<unknown>(`${scope}/fleets/${fleetId}`));
    if (fleet.name !== identifiers.name) throw new Error("The retained fleet name does not match; no records were deleted");
    await purgeFixtureFleet(handle, identifiers.workspace, fleetId);
  });
  for (const draft of identifiers.drafts) await allowMissing(() => client.delete(`${scope}/drafts/${draft}`));
}

/** A previous attempt may have deleted the record before another deletion failed. */
async function allowMissing(operation: () => Promise<void>): Promise<void> {
  try {await operation();}
  catch (error) {
    if (!(error instanceof FixtureHttpError && error.status === HTTP_STATUS_NOT_FOUND)) throw error;
  }
}

if (import.meta.main) {
  loadWorktreeEnv();
  const path = process.argv[2];
  if (!path) throw new Error("Usage: bun tests/e2e/acceptance/fixtures/continuity-cleanup.ts <identifiers.json>");
  const file = await stat(path);
  if (file.size > MAX_MANIFEST_BYTES) throw new Error("The continuity identifier attachment exceeds its size limit");
  await cleanupContinuity(JSON.parse(await readFile(path, "utf8")));
}
