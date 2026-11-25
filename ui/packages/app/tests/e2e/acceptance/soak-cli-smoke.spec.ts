import { rm, writeFile } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { v7 as uuidv7 } from "uuid";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import { FleetDetailSchema } from "@/lib/api/fleet-detail";
import { FIXTURE_KEY, JWT_CACHE_PATH } from "./fixtures/constants";
import { ensureSecondWorkspace } from "./fixtures/seed";
import { cliEnv, deleteCliKey, makeCliStateDir, mintCliKey, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";
import { clientFor } from "./fixtures/api-client";
import { awaitFirstTurnSettled, runtimeModelRef } from "./fixtures/fleet-create";
import { runtimeHistory } from "./fixtures/soak-history";
import { SOAK_FLEET_PREFIX, firstMessage, runtimeMessage } from "./fixtures/soak-scenario";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { uniqueTag } from "./fixtures/observation";

const WORKSPACE_NAME = "fixture-soak";
const RUNTIME_TURNS = 20;
const TURN_TIMEOUT_MS = 300_000;
const cacheSchema = z.record(z.string(), z.object({sessionJwt: z.string()}));
// `fleet create --json` prints what the create answered: the fleet and its first turn.
const Created = z.object({fleet_id: z.uuidv7(), turn_id: z.uuidv7()});
const Steered = z.object({message: z.string()});

type Turn = {stage: "create" | "runtime"; turn: number; input: string; sentAt: string; doneAt?: string;
  reference: Record<string, string | null>; reply?: string; state?: string; exit?: number; stderr?: string};

test("command line: a fleet from its first message, then 20 messages", async () => {
  test.setTimeout(60 * 60_000);
  const api = process.env.NEXT_PUBLIC_API_URL;
  if (!api) throw new Error("NEXT_PUBLIC_API_URL is required");
  const jwt = cacheSchema.parse(JSON.parse(readFileSync(JWT_CACHE_PATH, "utf8")))[FIXTURE_KEY.regular]?.sessionJwt;
  if (!jwt) throw new Error("Regular fixture session is missing");
  const {id: workspace} = await ensureSecondWorkspace(FIXTURE_KEY.regular, WORKSPACE_NAME);
  const key = await mintCliKey(api, jwt, `soak-cli-${uuidv7()}`);
  const {root, stateDir} = await makeCliStateDir("soak-cli-");
  const base = `/v1/workspaces/${workspace}`;
  const name = `${SOAK_FLEET_PREFIX}cli-${uniqueTag()}`;
  const turns: Turn[] = [];
  const evidence = {workspace, name, api, fleet: null as string | null, whoami: null as unknown, turns, verdict: "incomplete"};
  const record = () => writeFile(test.info().outputPath("cli-smoke.json"), JSON.stringify(evidence, null, 2));
  let passed = false;
  try {
    await writeCliState(stateDir, workspace, key.key, api, WORKSPACE_NAME);
    const client = clientFor({sessionJwt: key.key});
    const env = cliEnv({AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: api, AGENTSFLEET_API_KEY: key.key});
    const who = await spawnAgentsfleet(["--json", "whoami"], env);
    evidence.whoami = {exit: who.code, stdout: who.stdout.trim()};
    expect(who.code, who.stderr).toBe(0);
    const model = await runtimeModelRef({sessionJwt: key.key}, workspace);

    // One message makes the fleet and its first turn.
    const first = firstMessage(name);
    const createTurn: Turn = {stage: "create", turn: 0, input: first, sentAt: new Date().toISOString(), reference: {fleet: null, turn: null}};
    turns.push(createTurn);
    const created = await spawnAgentsfleet(["--json", "fleet", "create", first, "--name", name, "--model", model], env, TURN_TIMEOUT_MS);
    createTurn.exit = created.code; createTurn.doneAt = new Date().toISOString();
    if (created.code !== 0) createTurn.stderr = created.stderr.slice(-4000);
    expect(created.code, created.stderr).toBe(0);
    const answer = Created.parse(JSON.parse(created.stdout));
    evidence.fleet = answer.fleet_id;
    createTurn.reference = {fleet: answer.fleet_id, turn: answer.turn_id};
    const stored = FleetDetailSchema.parse(await client.get<unknown>(`${base}/fleets/${evidence.fleet}`));
    expect(stored.name).toBe(name);
    expect(stored.status).toBe("active");
    await awaitFirstTurnSettled({sessionJwt: key.key}, workspace, answer.fleet_id, answer.turn_id);
    console.log(JSON.stringify({cli: "create", fleet: evidence.fleet, status: "PASS"}));

    for (let index = 0; index < RUNTIME_TURNS; index++) {
      const input = `For project ${name}: ${runtimeMessage(index + 1)}`;
      const turn: Turn = {stage: "runtime", turn: index + 1, input, sentAt: new Date().toISOString(),
        reference: {fleet: evidence.fleet, turn: null}};
      turns.push(turn);
      const steered = await spawnAgentsfleet(["--json", "fleet", "steer", evidence.fleet!, input], env, TURN_TIMEOUT_MS);
      turn.exit = steered.code; turn.doneAt = new Date().toISOString();
      if (steered.code !== 0) turn.stderr = steered.stderr.slice(-4000);
      const rows = (await runtimeHistory(client, `${base}/fleets/${evidence.fleet}`)).filter((row) => row.event_type === "chat");
      const saved = rows.find((row) => Steered.safeParse(JSON.parse(row.request_json)).data?.message === input);
      turn.reference.turn = saved?.turn_id ?? null; turn.reply = saved?.response_text ?? undefined; turn.state = saved?.status;
      expect(steered.code, `runtime turn ${index + 1} exit: ${steered.stderr}`).toBe(0);
      expect(saved?.status, `runtime turn ${index + 1}: ${saved?.failure_label ?? ""}`).toBe("processed");
      expect(saved?.response_text?.trim().length, `runtime turn ${index + 1} reply text`).toBeGreaterThan(0);
      console.log(JSON.stringify({cli: "runtime", turn: index + 1, turn_id: saved?.turn_id, status: "PASS"}));
    }

    // History keeps every input once, the first message's included, each with its own turn.
    const runtimeRows = (await runtimeHistory(client, `${base}/fleets/${evidence.fleet}`)).filter((row) => row.event_type === "chat");
    expect(runtimeRows).toHaveLength(RUNTIME_TURNS + 1);
    expect(new Set(runtimeRows.map((row) => row.turn_id)).size).toBe(RUNTIME_TURNS + 1);
    expect(runtimeRows.map((row) => Steered.parse(JSON.parse(row.request_json)).message).toSorted())
      .toEqual(turns.map((turn) => turn.input).toSorted());
    evidence.verdict = "pass";
    passed = true;
  } catch (error) {
    evidence.verdict = `fail: ${String(error)}`;
    throw error;
  } finally {
    await record();
    // A failed run keeps its fleet live for inspection; the key never outlives the run.
    if (passed && evidence.fleet) await purgeFixtureFleet({sessionJwt: key.key}, workspace, evidence.fleet);
    await deleteCliKey(api, jwt, key.id);
    await rm(root, {recursive: true, force: true});
  }
});
