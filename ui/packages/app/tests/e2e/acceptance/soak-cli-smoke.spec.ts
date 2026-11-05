import { rm, writeFile } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { v7 as uuidv7 } from "uuid";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import { FleetSchema, MESSAGE_ROLE, MESSAGE_STATE, ReviewSchema } from "@/lib/api/draft-types";
import { FIXTURE_KEY, JWT_CACHE_PATH } from "./fixtures/constants";
import { ensureSecondWorkspace } from "./fixtures/seed";
import { cliEnv, deleteCliKey, makeCliStateDir, mintCliKey, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";
import { clientFor } from "./fixtures/api-client";
import { authoringModelRef } from "./fixtures/native-setup";
import { draftHistory, runtimeHistory } from "./fixtures/soak-history";
import { SOAK_FLEET_PREFIX, initialSetupMessage, runtimeMessage, setupMessage } from "./fixtures/soak-scenario";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { uniqueTag } from "./fixtures/observation";

const WORKSPACE_NAME = "fixture-soak";
const SETUP_TURNS = 20;
const RUNTIME_TURNS = 20;
const TURN_TIMEOUT_MS = 300_000;
const cacheSchema = z.record(z.string(), z.object({sessionJwt: z.string()}));
const terminalFleetSchema = FleetSchema.omit({webhook_urls: true});
const Steered = z.object({message: z.string()});

type Turn = {stage: "setup" | "runtime"; turn: number; input: string; sentAt: string; doneAt?: string;
  reference: Record<string, string | null>; reply?: string; state?: string; exit?: number; stderr?: string};

function jsonLines(text: string): unknown[] {
  return text.trim().split("\n").filter(Boolean).flatMap((line) => {
    try { return [JSON.parse(line)]; } catch { return []; }
  });
}

test("command line: 20 setup messages, save, 20 runtime messages", async () => {
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
  const evidence = {workspace, name, api, draft: null as string | null, fleet: null as string | null,
    whoami: null as unknown, review: null as unknown, turns, verdict: "incomplete"};
  const record = () => writeFile(test.info().outputPath("cli-smoke.json"), JSON.stringify(evidence, null, 2));
  let passed = false;
  try {
    await writeCliState(stateDir, workspace, key.key, api, WORKSPACE_NAME);
    const client = clientFor({sessionJwt: key.key});
    const env = cliEnv({AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: api, AGENTSFLEET_API_KEY: key.key});
    const who = await spawnAgentsfleet(["--json", "whoami"], env);
    evidence.whoami = {exit: who.code, stdout: who.stdout.trim()};
    expect(who.code, who.stderr).toBe(0);
    const model = await authoringModelRef({sessionJwt: key.key}, workspace);

    for (let index = 0; index < SETUP_TURNS; index++) {
      const input = index === 0 ? initialSetupMessage(name) : `For fleet ${name}: ${setupMessage(index, 1).text}`;
      const turn: Turn = {stage: "setup", turn: index + 1, input, sentAt: new Date().toISOString(),
        reference: {draft: evidence.draft, sequence: String(index + 1)}};
      turns.push(turn);
      const args = ["--json", "chat", ...(evidence.draft ? [evidence.draft] : ["--model", model]),
        "--no-input", "--operation-id", uuidv7(), "--message", input];
      const sent = await spawnAgentsfleet(args, env, TURN_TIMEOUT_MS);
      turn.exit = sent.code; turn.doneAt = new Date().toISOString();
      if (sent.code !== 0) turn.stderr = sent.stderr.slice(-4000);
      if (!evidence.draft) {
        const drafts = await client.get<{items: {id: string; title: string}[]}>(`${base}/drafts?limit=100`);
        evidence.draft = drafts.items.find((item) => item.title.includes(name))?.id ?? null;
        turn.reference.draft = evidence.draft;
      }
      expect(sent.code, `setup turn ${index + 1} exit: ${sent.stderr}`).toBe(0);
      expect(evidence.draft, "the terminal draft is discoverable").toBeTruthy();
      // Durable state decides, not the terminal's own rendering.
      const reply = (await draftHistory(client, `${base}/drafts/${evidence.draft}`))
        .find((row) => row.role === MESSAGE_ROLE.assistant && row.sequence === String(index + 1));
      turn.reply = reply?.text; turn.state = reply?.state;
      expect(reply?.failure, `setup turn ${index + 1} failure`).toBeUndefined();
      expect(reply?.state, `setup turn ${index + 1} state`).toBe(MESSAGE_STATE.completed);
      expect(reply?.text?.trim().length, `setup turn ${index + 1} reply text`).toBeGreaterThan(0);
      console.log(JSON.stringify({cli: "setup", turn: index + 1, status: "PASS"}));
    }

    const inspected = await spawnAgentsfleet(["--json", "chat", evidence.draft!, "--no-input", "--review", "--name", name], env);
    expect(inspected.code, inspected.stderr).toBe(0);
    const review = jsonLines(inspected.stdout).map((value) => ReviewSchema.safeParse(value)).find((result) => result.success);
    if (!review?.success) throw new Error("CLI returned no reviewed setup");
    evidence.review = {digest: review.data.review_digest};
    const saved = await spawnAgentsfleet(["--json", "chat", evidence.draft!, "--no-input", "--save", "--name", name,
      "--review-digest", review.data.review_digest], env, TURN_TIMEOUT_MS);
    expect(saved.code, saved.stderr).toBe(0);
    const fleet = jsonLines(saved.stdout).map((value) => terminalFleetSchema.safeParse(value)).find((value) => value.success);
    if (!fleet?.success) throw new Error(`CLI returned no saved fleet: ${saved.stdout.slice(-2000)}`);
    evidence.fleet = fleet.data.id;
    const stored = FleetSchema.parse(await client.get<unknown>(`${base}/fleets/${evidence.fleet}`));
    expect(stored.name).toBe(name);
    expect(stored.setup_revision).toBe("1");
    console.log(JSON.stringify({cli: "save", fleet: evidence.fleet, status: "PASS"}));

    for (let index = 0; index < RUNTIME_TURNS; index++) {
      const input = `For project ${name}: ${runtimeMessage(index + 1)}`;
      const turn: Turn = {stage: "runtime", turn: index + 1, input, sentAt: new Date().toISOString(),
        reference: {fleet: evidence.fleet, event: null}};
      turns.push(turn);
      const steered = await spawnAgentsfleet(["--json", "steer", evidence.fleet!, input], env, TURN_TIMEOUT_MS);
      turn.exit = steered.code; turn.doneAt = new Date().toISOString();
      if (steered.code !== 0) turn.stderr = steered.stderr.slice(-4000);
      const rows = (await runtimeHistory(client, `${base}/fleets/${evidence.fleet}`)).filter((row) => row.event_type === "chat");
      const event = rows.find((row) => Steered.safeParse(JSON.parse(row.request_json)).data?.message === input);
      turn.reference.event = event?.event_id ?? null; turn.reply = event?.response_text ?? undefined; turn.state = event?.status;
      expect(steered.code, `runtime turn ${index + 1} exit: ${steered.stderr}`).toBe(0);
      expect(event?.status, `runtime turn ${index + 1}: ${event?.failure_label ?? ""}`).toBe("processed");
      expect(event?.response_text?.trim().length, `runtime turn ${index + 1} reply text`).toBeGreaterThan(0);
      console.log(JSON.stringify({cli: "runtime", turn: index + 1, event: event?.event_id, status: "PASS"}));
    }

    // History keeps every input once, in order, with its own reply.
    const setupRows = await draftHistory(client, `${base}/drafts/${evidence.draft}`);
    expect(setupRows.filter((row) => row.role === MESSAGE_ROLE.user).map((row) => row.text))
      .toEqual(turns.filter((turn) => turn.stage === "setup").map((turn) => turn.input));
    expect(setupRows.filter((row) => row.role === MESSAGE_ROLE.assistant)).toHaveLength(SETUP_TURNS);
    const runtimeRows = (await runtimeHistory(client, `${base}/fleets/${evidence.fleet}`)).filter((row) => row.event_type === "chat");
    expect(runtimeRows).toHaveLength(RUNTIME_TURNS);
    expect(new Set(runtimeRows.map((row) => row.event_id)).size).toBe(RUNTIME_TURNS);
    evidence.verdict = "pass";
    passed = true;
  } catch (error) {
    evidence.verdict = `fail: ${String(error)}`;
    throw error;
  } finally {
    await record();
    // A failed run keeps its draft and fleet live for inspection; the key never outlives the run.
    if (passed && evidence.fleet) await purgeFixtureFleet({sessionJwt: key.key}, workspace, evidence.fleet);
    if (passed && evidence.draft) await clientFor({sessionJwt: key.key}).delete(`${base}/drafts/${evidence.draft}`);
    await deleteCliKey(api, jwt, key.id);
    await rm(root, {recursive: true, force: true});
  }
});
