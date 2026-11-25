// The journeys that need an app the fixture person connected: GitHub for
// watch-dog, Slack for chatty-parrot.
//
// A connection is one workspace's: a provider account routes to the workspace
// that connected it, and no fixture can make that connection, because it ends
// in the provider's own consent screen. So these journeys run in whichever
// workspace `connections show` reads as connected, beside the fleets other
// suites create there. They run only against the deployed development API: a
// local stack holds no connected app, and the production release run is not
// set up for them.

import assert from "node:assert/strict";
import { Schema } from "effect";

import { CONNECTOR_STATE } from "../../../src/services/connectors.ts";
import { EXIT_CODE } from "../../../src/errors/index.ts";
import { runFleetctl } from "./cli.js";
import { API_URL_DEV } from "./constants.ts";
import type { CreatedFleet } from "./fleet-create.ts";
import { refusalCode } from "./negatives.ts";
import { said, type PrimeFleet } from "./prime-fleets.ts";
import { LIVE_TARGET, removeFleet, type PrimeSession, type WorkspaceChoice } from "./prime-session.ts";
import { listWorkspaces } from "./workspace-ops.ts";

/** Whether this run targets the deployment these journeys run against. */
export const onDeployedDev = LIVE_TARGET === API_URL_DEV;
/** Why a journey here is not run against any other target, for its skip line. */
export const DEPLOYED_DEV_ONLY = `runs only against ${API_URL_DEV}`;

// af_core/src/error_code/fleet.rs: a name this workspace already holds.
const CODE_NAME_TAKEN = "UZ-AGT-006";
// The names tried before giving up, the fixture's own first and then `name-2`,
// `name-3`, ... as the browser suite numbers them. The bound only ends a loop a
// daemon refusing every name would never end.
const NAME_ATTEMPTS = 50;
const CREATE_TIMEOUT_MS = 120_000;

const ConnectionSchema = Schema.Struct({ state: Schema.String });
const CreatedSchema = Schema.Struct({ fleet_id: Schema.String, turn_id: Schema.String });

export interface NamedFleet extends CreatedFleet {
  /** The name the fleet was created under: the fixture's, or the fixture's with a counter. */
  readonly name: string;
}

/**
 * The fixture person's workspace that connected `provider`, read with
 * `connections show`. None connected fails the file, naming each workspace's state.
 */
export function connectedWorkspace(provider: string): WorkspaceChoice {
  return async (env) => {
    const states: string[] = [];
    for (const workspace of await listWorkspaces(env)) {
      const shown = await runFleetctl(
        ["connections", "show", provider, "--workspace", workspace.workspace_id, "--json"], { env });
      assert.equal(shown.code, 0, `connections show ${provider} exited ${shown.code}: ${shown.stderr}`);
      const state = Schema.decodeUnknownSync(ConnectionSchema)(JSON.parse(shown.stdout.trim())).state;
      if (state === CONNECTOR_STATE.connected) return workspace.workspace_id;
      states.push(`${workspace.workspace_id}: ${state}`);
    }
    throw new Error(`no workspace of the fixture person has connected ${provider} (${states.join("; ")}); ` +
      "a person connects it once from the dashboard's connected apps");
  };
}

/** The values of `names` from the environment. Any that is unset fails, by name and never by value. */
export function requireVariables<Name extends string>(names: ReadonlyArray<Name>): Record<Name, string> {
  const missing = names.filter((name) => !process.env[name]);
  if (missing.length > 0) throw new Error(`unset in the environment: ${missing.join(", ")}`);
  return Object.fromEntries(names.map((name) => [name, process.env[name] ?? ""])) as Record<Name, string>;
}

/**
 * Creates the fixture's fleet in a workspace other suites create fleets in.
 * A name already held there is another suite's fleet, so it is never deleted:
 * the fixture's name is tried with a counter after it instead.
 */
export async function primeFleetBeside(session: PrimeSession, fixture: PrimeFleet, added: string): Promise<NamedFleet> {
  const message = said(fixture.first_message, added);
  for (let attempt = 1; attempt <= NAME_ATTEMPTS; attempt += 1) {
    const name = attempt === 1 ? fixture.name : `${fixture.name}-${attempt}`;
    const result = await session.run(["--json", "fleet", "create", message, "--name", name], CREATE_TIMEOUT_MS);
    if (result.code === 0) {
      const created = Schema.decodeUnknownSync(CreatedSchema)(JSON.parse(result.stdout.trim()));
      session.onClose(() => removeFleet(session, created.fleet_id));
      return { ...created, name };
    }
    if (result.code !== EXIT_CODE.ServerError || refusalCode(result) !== CODE_NAME_TAKEN) {
      throw new Error(`fleet create exited ${result.code}: ${result.stderr.trim()}`);
    }
  }
  throw new Error(`every name from ${fixture.name} to ${fixture.name}-${NAME_ATTEMPTS} is taken in this workspace`);
}
