import { rm } from "node:fs/promises";
import { readFileSync } from "node:fs";
import { v7 as uuidv7 } from "uuid";
import { expect, test } from "@playwright/test";
import { z } from "zod";
import { ReviewSchema, FleetSchema, ModelsSchema } from "@/lib/api/draft-types";
import { signInAs } from "./fixtures/auth";
import { purgeFixtureFleet } from "./fixtures/teardown";
import { FIXTURE_KEY, JWT_CACHE_PATH } from "./fixtures/constants";
import { getDefaultWorkspaceId } from "./fixtures/seed";
import { cliEnv, deleteCliKey, makeCliStateDir, mintCliKey, spawnAgentsfleet, writeCliState } from "./fixtures/cli-runner";
import { agentDisplayName } from "@/lib/fleets/agent-label";
import { workspaceHref } from "./fixtures/nav";
import { clientFor } from "./fixtures/api-client";

const cacheSchema = z.record(z.string(), z.object({sessionJwt: z.string()}));
const FLOW_TIMEOUT_MS = 330_000;

function jsonLines(text: string): unknown[] {
  return text.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
}

test("terminal chat creates a private draft and explicitly saves a fleet visible in the browser", async ({page}) => {
  test.setTimeout(FLOW_TIMEOUT_MS);
  const api = process.env.NEXT_PUBLIC_API_URL;
  if (!api) throw new Error("NEXT_PUBLIC_API_URL is required");
  const cache = cacheSchema.parse(JSON.parse(readFileSync(JWT_CACHE_PATH, "utf8")));
  const jwt = cache[FIXTURE_KEY.regular]?.sessionJwt;
  if (!jwt) throw new Error("Regular fixture session is missing");
  const workspace = await getDefaultWorkspaceId(FIXTURE_KEY.regular);
  const key = await mintCliKey(api, jwt, `chat-cli-${uuidv7()}`);
  const {root, stateDir} = await makeCliStateDir("chat-cli-");
  const base = `/v1/workspaces/${workspace}`;
  const name = `cli-chat-${uuidv7().slice(-8)}`;
  let saved: string | undefined;
  try {
    await writeCliState(stateDir, workspace, key.key, api, "acceptance-workspace");
    const client = clientFor({sessionJwt: key.key});
    const env = cliEnv({AGENTSFLEET_STATE_DIR: stateDir, AGENTSFLEET_API_URL: api});
    const models = ModelsSchema.parse(await client.get<unknown>(`${base}/models?limit=100`));
    const model = models.items.find((item) => item.eligibility.authoring);
    if (!model) throw new Error("No authoring model for terminal chat acceptance");
    const sent = await spawnAgentsfleet(["--json", "chat", "--no-input", "--model", model.ref,
      "--operation-id", uuidv7(), "--message", `${name}: Create a fleet for chat and steer messages only, with no external triggers, tools or credentials, daily budget one dollar, no monthly ceiling. Reply briefly to messages.`], env, FLOW_TIMEOUT_MS);
    expect(sent.code, sent.stderr).toBe(0);
    const drafts = await client.get<{items: {id: string; title: string}[]}>(`${base}/drafts?limit=100`);
    const draft = drafts.items.find((item) => item.title.includes(name));
    if (!draft) throw new Error("Terminal-created private draft was not discoverable by its creator");
    const inspected = await spawnAgentsfleet(["--json", "chat", draft.id, "--no-input", "--review", "--name", name], env);
    expect(inspected.code, inspected.stderr).toBe(0);
    const review = jsonLines(inspected.stdout).map((value) => ReviewSchema.safeParse(value)).find((result) => result.success);
    if (!review?.success) throw new Error("CLI returned no reviewed setup");
    const result = await spawnAgentsfleet(["--json", "chat", draft.id, "--no-input", "--save", "--review-digest", review.data.review_digest], env);
    expect(result.code, result.stderr).toBe(0);
    const fleet = jsonLines(result.stdout).map((value) => FleetSchema.safeParse(value)).find((value) => value.success);
    if (!fleet?.success) throw new Error("CLI returned no saved fleet");
    saved = fleet.data.id;
    await signInAs(page, FIXTURE_KEY.regular);
    await page.goto(workspaceHref(workspace, `fleets/${saved}`));
    await expect(page.getByRole("heading", {name: agentDisplayName(saved), exact: true})).toBeVisible();
  } finally {
    if (saved) await purgeFixtureFleet({sessionJwt: key.key}, workspace, saved);
    await deleteCliKey(api, jwt, key.id);
    await rm(root, {recursive: true, force: true});
  }
});
