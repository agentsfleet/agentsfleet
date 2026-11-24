import { rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { v7 as uuidv7 } from "uuid";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { signInAs } from "./fixtures/auth";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { FIXTURE_KEY, JWT_CACHE_PATH } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { cliEnv, deleteCliKey, makeCliStateDir, mintCliKey, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";
import { workspaceHref } from "./fixtures/nav";
import { clientFor } from "./fixtures/api-client";
import { runtimeModelRef } from "./fixtures/fleet-create";

const cacheSchema = z.record(z.string(), z.object({sessionJwt: z.string()}));
const FLOW_TIMEOUT_MS = 330_000;
const MESSAGE = "watch checkout errors";
// `fleet create --json` prints what the create answered: the fleet and its first turn.
const createdSchema = z.object({fleet_id: z.uuidv7(), turn_id: z.uuidv7()});

test("fleet create makes a fleet from one message that the browser shows", async ({page}) => {
  test.setTimeout(FLOW_TIMEOUT_MS);
  const api = process.env.NEXT_PUBLIC_API_URL;
  if (!api) throw new Error("NEXT_PUBLIC_API_URL is required");
  const cache = cacheSchema.parse(JSON.parse(readFileSync(JWT_CACHE_PATH, "utf8")));
  const jwt = cache[FIXTURE_KEY.regular]?.sessionJwt;
  if (!jwt) throw new Error("Regular fixture session is missing");
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const key = await mintCliKey(api, jwt, `fleet-cli-${uuidv7()}`);
  const {root, stateDir} = await makeCliStateDir("fleet-cli-");
  const name = `cli-fleet-${uuidv7().slice(-8)}`;
  let created: string | undefined;
  try {
    await writeCliState(stateDir, workspace, key.key, api, "acceptance-workspace");
    const client = clientFor({sessionJwt: key.key});
    const env = cliEnv({AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: api, AGENTSFLEET_API_KEY: key.key});
    const model = await runtimeModelRef({sessionJwt: key.key}, workspace);
    const result = await spawnAgentsfleet(["--json", "fleet", "create", MESSAGE, "--name", name, "--model", model], env, FLOW_TIMEOUT_MS);
    expect(result.code, result.stdout + result.stderr).toBe(0);
    const answer = createdSchema.parse(JSON.parse(result.stdout));
    created = answer.fleet_id;
    const fleet = FleetDetailSchema.parse(await client.get<unknown>(`/v1/workspaces/${workspace}/fleets/${created}`));
    expect(fleet).toMatchObject({name, status: "active", model_ref: model});
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspace, `fleets/${created}`));
    await expect(page.getByRole("heading", {name, exact: true})).toBeVisible();
    await expect(page.getByLabel("Fleet chat").getByText(MESSAGE, {exact: true}).first()).toBeVisible();
  } finally {
    if (created) await purgeFixtureFleet({sessionJwt: key.key}, workspace, created);
    await deleteCliKey(api, jwt, key.id);
    await rm(root, {recursive: true, force: true});
  }
});
