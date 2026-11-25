// A live spec's person and workspace, and the prime fleets it creates there.
//
// One call per spec file. The person signs in as in every live spec: a session
// minted through the identity provider's admin API, exchanged for the CLI's
// own credential on disk. The file then works in a workspace no other file
// writes to, because a fleet's name is unique in its workspace and several
// files create the same prime fleet while the lane runs them side by side.

import { afterAll, beforeAll } from "bun:test";
import assert from "node:assert/strict";
import fs from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import { Schema } from "effect";

import { API_URL_ENV, NO_COLOR_ENV, STATE_DIR_ENV } from "../../../src/constants/env.ts";
import { resolveAcceptanceEnv, resolveClerkSecret, resolveFixtureEmail } from "../global-setup.ts";
import { attachJwt } from "./clerk-admin.ts";
import { composeEnv, runFleetctl } from "./cli.js";
import type { RunResult } from "./cli.js";
import { ACCEPTANCE_TARGET_ENV } from "./constants.ts";
import { createFleetFromCli, type CreatedFleet } from "./fleet-create.ts";
import { assertNoSecretLeak, refusalCode } from "./negatives.ts";
import { said, type PrimeFleet } from "./prime-fleets.ts";
import { hydrateWorkspacesForToken } from "./workspace-hydration.ts";
import { addWorkspace, listWorkspaces, useWorkspace } from "./workspace-ops.ts";

type Env = Readonly<Record<string, string>>;

/** The API the live lane was pointed at; empty outside it. */
export const LIVE_TARGET = process.env[ACCEPTANCE_TARGET_ENV] ?? "";
/** Whether this run is the live lane's: a spec registers its journey only then. */
export const isLive = LIVE_TARGET.startsWith("https://");

const STATE_DIR_PREFIX = "agentsfleet-prime-";
const NO_COLOR_ON = "1";
const JSON_FLAG = "--json";
// A create returns once the fleet and its first turn are written; the run is not waited for.
const CREATE_TIMEOUT_MS = 120_000;
// Teardown deletes each fleet and secret the file made, and collects a send left running.
const CLOSE_TIMEOUT_MS = 180_000;
// af_core/src/error_code/fleet.rs: no fleet with that id lives in this workspace.
const CODE_FLEET_NOT_FOUND = "UZ-AGT-009";

const FleetListSchema = Schema.Struct({
  items: Schema.Array(Schema.Struct({ id: Schema.String, name: Schema.String })),
});

/** Picks the workspace a spec works in, and answers its id. */
export type WorkspaceChoice = (env: Env) => Promise<string>;

export interface PrimeSession {
  /** The CLI as the fixture person. Output carrying the session token fails the call. */
  readonly run: (args: ReadonlyArray<string>, timeoutMs?: number) => Promise<RunResult>;
  /** `run` with `--json`, refusing anything but exit 0, parsed. */
  readonly json: (args: ReadonlyArray<string>, timeoutMs?: number) => Promise<unknown>;
  readonly env: () => Env;
  readonly stateDir: () => string;
  readonly token: () => string;
  /** Adds a step to the file's teardown. Steps run newest first. */
  readonly onClose: (step: () => Promise<void>) => void;
}

/**
 * The workspace named `name`, created the first time a spec asks for it.
 *
 * The name is fixed and never carries a run's identifier: no route deletes a
 * workspace, so a name per run would leave one behind every time.
 */
export function ownWorkspace(name: string): WorkspaceChoice {
  return async (env) => {
    const held = (await listWorkspaces(env)).find((row) => row.name === name);
    return held?.workspace_id ?? (await addWorkspace(env, name)).workspaceId;
  };
}

/** Opens the file's session before its tests and closes it after them. */
export function primeSession(choose: WorkspaceChoice): PrimeSession {
  let sessionJwt = "";
  let stateDir = "";
  let env: Record<string, string> = {};
  const closers: Array<() => Promise<void>> = [];

  async function run(args: ReadonlyArray<string>, timeoutMs?: number): Promise<RunResult> {
    const result = await runFleetctl(args, timeoutMs === undefined ? { env } : { env, timeoutMs });
    assertNoSecretLeak(result, sessionJwt);
    return result;
  }

  async function json(args: ReadonlyArray<string>, timeoutMs?: number): Promise<unknown> {
    const result = await run([...args, JSON_FLAG], timeoutMs);
    assert.equal(result.code, 0, `${args.join(" ")} exited ${result.code}: ${result.stderr || result.stdout}`);
    return JSON.parse(result.stdout.trim());
  }

  beforeAll(async () => {
    const apiUrl = resolveAcceptanceEnv().apiUrl;
    sessionJwt = (await attachJwt(resolveClerkSecret(), { email: resolveFixtureEmail("regular") })).sessionJwt;
    stateDir = await fs.mkdtemp(path.join(os.tmpdir(), STATE_DIR_PREFIX));
    env = composeEnv({ [API_URL_ENV]: apiUrl, [STATE_DIR_ENV]: stateDir, [NO_COLOR_ENV]: NO_COLOR_ON });
    await hydrateWorkspacesForToken({ apiUrl, token: sessionJwt, stateDir });
    await useWorkspace(env, await choose(env));
  });

  afterAll(async () => {
    const failures: unknown[] = [];
    for (const step of closers.reverse()) {
      try {
        await step();
      } catch (error: unknown) {
        failures.push(error);
      }
    }
    if (stateDir) await fs.rm(stateDir, { recursive: true, force: true });
    if (failures.length > 0) {
      throw new AggregateError(failures, `the session left something it created behind: ${failures.map(String).join("; ")}`);
    }
  }, CLOSE_TIMEOUT_MS);

  return {
    run,
    json,
    env: () => env,
    stateDir: () => stateDir,
    token: () => sessionJwt,
    onClose: (step) => { closers.push(step); },
  };
}

/** The ids of the fleets this workspace holds under `name`. */
export async function fleetsNamed(session: PrimeSession, name: string): Promise<ReadonlyArray<string>> {
  const page = Schema.decodeUnknownSync(FleetListSchema)(await session.json(["fleet", "list"]));
  return page.items.filter((fleet) => fleet.name === name).map((fleet) => fleet.id);
}

/** Deletes one fleet. A fleet a test already deleted is gone, which is what teardown wants. */
export async function removeFleet(session: PrimeSession, fleetId: string): Promise<void> {
  const removed = await session.run(["fleet", "delete", fleetId, JSON_FLAG]);
  if (removed.code === 0 || refusalCode(removed) === CODE_FLEET_NOT_FOUND) return;
  throw new Error(`fleet delete ${fleetId} exited ${removed.code}: ${removed.stderr.trim()}`);
}

/**
 * Deletes the fleets an interrupted run left under `name`. The name is unique
 * in the workspace and the workspace is this file's alone, so a leftover is
 * never another file's fleet, and it would refuse every later create.
 */
export async function removeFleetsNamed(session: PrimeSession, name: string): Promise<void> {
  for (const stale of await fleetsNamed(session, name)) await removeFleet(session, stale);
}

/**
 * Creates the fixture's fleet under its own name with `fleet create`, its first
 * message followed by `added` when the journey has a value of this run's to
 * give. The fleet replaces a leftover of that name and is deleted when the
 * file ends.
 */
export async function primeFleet(session: PrimeSession, fixture: PrimeFleet, added?: string): Promise<CreatedFleet> {
  await removeFleetsNamed(session, fixture.name);
  const message = added === undefined ? fixture.first_message : said(fixture.first_message, added);
  const created = await createFleetFromCli(session.env(), { name: fixture.name, message, timeoutMs: CREATE_TIMEOUT_MS });
  session.onClose(() => removeFleet(session, created.fleet_id));
  return created;
}

/**
 * Saves a workspace secret the fixture names, and deletes it when the file
 * ends. `secrets create` keeps a secret an interrupted run left under the name.
 */
export async function storeSecret(session: PrimeSession, name: string, data: Readonly<Record<string, string>>): Promise<void> {
  await session.json(["secrets", "create", name, "--data", JSON.stringify(data)]);
  session.onClose(async () => { await session.json(["secrets", "delete", name]); });
}
